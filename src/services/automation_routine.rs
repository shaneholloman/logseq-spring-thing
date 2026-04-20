//! Automation routine aggregate (design 03 §5).
//!
//! A `Routine` is the pod-resident spec for a scheduled automation.
//! It is loaded by the `AutomationOrchestratorActor` from
//! `/private/automations/{routine-id}.json`. Every routine carries a
//! NIP-26 `delegated_cap_id`; the orchestrator validates the cap at
//! each tick.
//!
//! The aggregate exposes:
//!
//! - `next_fire_at(now)` — the next scheduled fire time for the
//!   time-wheel scheduler (supports cron-like patterns: fixed-minute,
//!   daily, weekly-by-dow, and a raw interval-seconds form)
//! - `validate(&self)` — invariants required by §5.1 (cap expiry
//!   envelopes the routine, output-target/kind consistency, data_scopes
//!   subset of cap)
//! - `is_active_now(now)` — cap-expiry + `active` flag + not-past-expiry
//!
//! Cron parsing is intentionally minimal (no external `cron` crate
//! required). Production can swap in `croner` behind the same
//! `next_fire_at` API.

use chrono::{DateTime, Datelike, Duration as ChronoDuration, TimeZone, Timelike, Utc, Weekday};
use serde::{Deserialize, Serialize};

/// Output target for a routine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutputTarget {
    Inbox { path: String },
    PodPath { path: String },
    GraphMutation,
}

impl OutputTarget {
    pub fn validate_consistency(&self) -> Result<(), RoutineError> {
        match self {
            OutputTarget::Inbox { path } => {
                if !path.starts_with("/inbox/") {
                    return Err(RoutineError::OutputMismatch(
                        "kind=inbox requires /inbox/* path".into(),
                    ));
                }
            }
            OutputTarget::PodPath { path } => {
                if !(path.starts_with("/private/") || path.starts_with("/shared/")) {
                    return Err(RoutineError::OutputMismatch(
                        "kind=pod_path requires /private/** or /shared/**".into(),
                    ));
                }
            }
            OutputTarget::GraphMutation => {}
        }
        Ok(())
    }

    pub fn inbox_ns(&self) -> Option<&str> {
        match self {
            OutputTarget::Inbox { path } => path
                .strip_prefix("/inbox/")
                .and_then(|r| r.split('/').next()),
            _ => None,
        }
    }
}

/// Routine trigger — time (cron), graph event, ontology event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RoutineTrigger {
    /// Cron-like — see [`CronExpr`] for parseable shapes.
    Time { cron: String, tz: String },
    /// Backend event firehose.
    GraphEvent { event: String },
    /// Broker/ontology event with optional filter.
    OntologyEvent {
        event: String,
        #[serde(default)]
        filter: serde_json::Value,
    },
}

/// Tool + data scopes the routine may touch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutinePermissions {
    pub tool_scopes: Vec<String>,
    pub data_scopes: Vec<String>,
}

/// Action — skill invocation payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutineAction {
    pub skill_id: String,
    pub version: String,
    #[serde(default)]
    pub parameters: serde_json::Value,
}

/// Quiet-hours window (default Europe/London 22:00–07:00).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuietHours {
    pub tz: String,
    pub start: String,
    pub end: String,
}

/// Aggregate root — one entry in `/private/automations/`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Routine {
    pub routine_id: String,
    pub owner_webid: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub trigger: RoutineTrigger,
    pub action: RoutineAction,
    pub output_target: OutputTarget,
    pub permissions: RoutinePermissions,
    pub delegated_cap_id: String,
    pub delegated_cap_expiry: DateTime<Utc>,
    #[serde(default = "default_active")]
    pub active: bool,
    #[serde(default)]
    pub quiet_hours: Option<QuietHours>,
    #[serde(default = "default_max_runs")]
    pub max_runs_per_day: u32,
    #[serde(default = "default_failure_threshold")]
    pub consecutive_failure_suspend_threshold: u32,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
}

fn default_active() -> bool { true }
fn default_max_runs() -> u32 { 3 }
fn default_failure_threshold() -> u32 { 3 }
fn default_schema_version() -> String { "1.0".to_string() }

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RoutineError {
    #[error("routine expired")]
    Expired,
    #[error("routine inactive")]
    Inactive,
    #[error("cap expiry ({cap}) must be <= routine expiry ({routine})")]
    CapExpiryOutOfEnvelope { cap: String, routine: String },
    #[error("permissions.data_scopes must be a subset of cap.data_scopes")]
    DataScopeNotSubset,
    #[error("output_target kind/path mismatch: {0}")]
    OutputMismatch(String),
    #[error("unparseable cron expression: {0}")]
    BadCron(String),
    #[error("unsupported trigger for scheduler: {0}")]
    UnsupportedTrigger(String),
}

impl Routine {
    /// Run the §5.1 structural invariants — does NOT check the cap's
    /// live signature (that's [`crate::services::nip26_cap`]'s job).
    pub fn validate_invariants(&self, cap_data_scopes: &[String]) -> Result<(), RoutineError> {
        // cap TTL envelope
        if self.delegated_cap_expiry > self.expires_at {
            return Err(RoutineError::CapExpiryOutOfEnvelope {
                cap: self.delegated_cap_expiry.to_rfc3339(),
                routine: self.expires_at.to_rfc3339(),
            });
        }
        // data_scopes subset
        for req in &self.permissions.data_scopes {
            if !cap_data_scopes.iter().any(|c| scopes_match(req, c)) {
                return Err(RoutineError::DataScopeNotSubset);
            }
        }
        self.output_target.validate_consistency()?;
        Ok(())
    }

    /// Is the routine past its declared `expires_at`?
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires_at
    }

    /// Should the scheduler consider this routine right now?
    pub fn is_eligible(&self, now: DateTime<Utc>) -> bool {
        self.active && !self.is_expired(now)
    }

    /// Next fire time — None for non-time triggers (those are pushed
    /// by the event bus, not the time wheel).
    pub fn next_fire_at(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match &self.trigger {
            RoutineTrigger::Time { cron, .. } => CronExpr::parse(cron)
                .ok()
                .and_then(|c| c.next_after(now)),
            _ => None,
        }
    }

    /// Is `now` inside quiet hours? Assumes times are stored as
    /// `HH:MM` and the TZ is validated elsewhere; we interpret them as
    /// UTC wall-clock for the MVP (the orchestrator applies a tz
    /// conversion when it loads the routine in prod).
    pub fn is_quiet(&self, now: DateTime<Utc>) -> bool {
        let Some(qh) = &self.quiet_hours else { return false };
        let (Ok(start), Ok(end)) = (parse_hm(&qh.start), parse_hm(&qh.end)) else {
            return false;
        };
        let (h, m) = (now.hour() as u16, now.minute() as u16);
        let now_min = h * 60 + m;
        let s_min = start.0 * 60 + start.1;
        let e_min = end.0 * 60 + end.1;
        if s_min <= e_min {
            now_min >= s_min && now_min < e_min
        } else {
            // Wraparound (22:00–07:00)
            now_min >= s_min || now_min < e_min
        }
    }
}

fn parse_hm(s: &str) -> Result<(u16, u16), ()> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err(());
    }
    let h: u16 = parts[0].parse().map_err(|_| ())?;
    let m: u16 = parts[1].parse().map_err(|_| ())?;
    if h >= 24 || m >= 60 {
        return Err(());
    }
    Ok((h, m))
}

/// Subset test for scope globs: `req ⊆ have` when `have`'s prefix
/// covers `req`'s prefix.
fn scopes_match(req: &str, have: &str) -> bool {
    let r = req.trim_end_matches('*');
    let h = have.trim_end_matches('*');
    r.starts_with(h)
}

/// Minimal cron-expression parser — supports:
///
/// - `"@every Ns"` interval form
/// - `"M H * * *"` daily at H:M UTC
/// - `"M H * * DOW"` weekly (DOW = 0–6, Sun=0, MON/TUE aliases)
/// - `"M H * * 1-5"` weekdays (Mon–Fri)
///
/// No external dependency; adequate for the three example routines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CronExpr {
    Every(ChronoDuration),
    Daily { hour: u32, minute: u32 },
    Weekly { hour: u32, minute: u32, weekdays: Vec<Weekday> },
}

impl CronExpr {
    pub fn parse(expr: &str) -> Result<Self, RoutineError> {
        let expr = expr.trim();
        if let Some(rest) = expr.strip_prefix("@every ") {
            // @every 30s, @every 5m, @every 2h
            let (num, unit) = rest.split_at(rest.len() - 1);
            let n: i64 = num.parse().map_err(|_| RoutineError::BadCron(expr.into()))?;
            let dur = match unit {
                "s" => ChronoDuration::seconds(n),
                "m" => ChronoDuration::minutes(n),
                "h" => ChronoDuration::hours(n),
                _ => return Err(RoutineError::BadCron(expr.into())),
            };
            return Ok(CronExpr::Every(dur));
        }
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(RoutineError::BadCron(expr.into()));
        }
        // Minute, hour are literal digits.
        let minute: u32 = parts[0].parse().map_err(|_| RoutineError::BadCron(expr.into()))?;
        let hour: u32 = parts[1].parse().map_err(|_| RoutineError::BadCron(expr.into()))?;
        if parts[2] != "*" || parts[3] != "*" {
            // Only day-of-month=* and month=* are supported for MVP.
            return Err(RoutineError::BadCron(format!(
                "only day-of-month=* month=* supported: {}",
                expr
            )));
        }
        if parts[4] == "*" {
            return Ok(CronExpr::Daily { hour, minute });
        }
        let weekdays = parse_dow(parts[4])?;
        Ok(CronExpr::Weekly {
            hour,
            minute,
            weekdays,
        })
    }

    pub fn next_after(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            CronExpr::Every(d) => Some(now + *d),
            CronExpr::Daily { hour, minute } => {
                let today = Utc
                    .with_ymd_and_hms(now.year(), now.month(), now.day(), *hour, *minute, 0)
                    .single()?;
                if today > now {
                    Some(today)
                } else {
                    Some(today + ChronoDuration::days(1))
                }
            }
            CronExpr::Weekly {
                hour,
                minute,
                weekdays,
            } => {
                // Search the next 7 days for a match.
                for day_ahead in 0..=7 {
                    let candidate_date = now.date_naive() + ChronoDuration::days(day_ahead);
                    let wd = candidate_date.weekday();
                    if !weekdays.contains(&wd) {
                        continue;
                    }
                    let candidate = Utc
                        .with_ymd_and_hms(
                            candidate_date.year(),
                            candidate_date.month(),
                            candidate_date.day(),
                            *hour,
                            *minute,
                            0,
                        )
                        .single()?;
                    if candidate > now {
                        return Some(candidate);
                    }
                }
                None
            }
        }
    }
}

fn parse_dow(s: &str) -> Result<Vec<Weekday>, RoutineError> {
    let mut out = Vec::new();
    for tok in s.split(',') {
        if let Some((a, b)) = tok.split_once('-') {
            let start = dow_token(a)?;
            let end = dow_token(b)?;
            let mut cur = start;
            out.push(cur);
            while cur != end {
                cur = cur.succ();
                out.push(cur);
            }
        } else {
            out.push(dow_token(tok)?);
        }
    }
    Ok(out)
}

fn dow_token(s: &str) -> Result<Weekday, RoutineError> {
    match s.trim() {
        "0" | "SUN" | "Sun" | "sun" => Ok(Weekday::Sun),
        "1" | "MON" | "Mon" | "mon" => Ok(Weekday::Mon),
        "2" | "TUE" | "Tue" | "tue" => Ok(Weekday::Tue),
        "3" | "WED" | "Wed" | "wed" => Ok(Weekday::Wed),
        "4" | "THU" | "Thu" | "thu" => Ok(Weekday::Thu),
        "5" | "FRI" | "Fri" | "fri" => Ok(Weekday::Fri),
        "6" | "SAT" | "Sat" | "sat" => Ok(Weekday::Sat),
        other => Err(RoutineError::BadCron(format!("bad dow token: {}", other))),
    }
}

// -----------------------------------------------------------------
// Example routines from spec §5.4
// -----------------------------------------------------------------

/// (a) Daily research brief — `0 8 * * 1-5`, inbox-only.
pub fn example_research_brief(owner_webid: &str, cap_id: &str) -> Routine {
    let now = Utc::now();
    Routine {
        routine_id: "rt-001".into(),
        owner_webid: owner_webid.into(),
        name: "Daily research brief".into(),
        description: "Assemble a brief of the last 24h of graph activity.".into(),
        trigger: RoutineTrigger::Time {
            cron: "0 8 * * 1-5".into(),
            tz: "Europe/London".into(),
        },
        action: RoutineAction {
            skill_id: "urn:skill:research-brief".into(),
            version: "1.3.0".into(),
            parameters: serde_json::json!({ "graph_view": "active-projects", "depth_hops": 2 }),
        },
        output_target: OutputTarget::Inbox {
            path: "/inbox/research-brief/".into(),
        },
        permissions: RoutinePermissions {
            tool_scopes: vec!["studio_context_assemble".into(), "ontology_discover".into()],
            data_scopes: vec![
                "pod:/private/kg/*".into(),
                "pod:/private/agent-memory/*".into(),
            ],
        },
        delegated_cap_id: cap_id.into(),
        delegated_cap_expiry: now + ChronoDuration::hours(24),
        active: true,
        quiet_hours: Some(QuietHours {
            tz: "Europe/London".into(),
            start: "22:00".into(),
            end: "07:00".into(),
        }),
        max_runs_per_day: 3,
        consecutive_failure_suspend_threshold: 3,
        created_at: now,
        expires_at: now + ChronoDuration::days(183),
        schema_version: "1.0".into(),
    }
}

/// (b) Weekly stale-node sweep — graph-event-triggered, writes to pod path.
pub fn example_stale_sweep(owner_webid: &str, cap_id: &str) -> Routine {
    let now = Utc::now();
    Routine {
        routine_id: "rt-002".into(),
        owner_webid: owner_webid.into(),
        name: "Weekly stale-node sweep".into(),
        description: "Report on nodes untouched > threshold_days".into(),
        trigger: RoutineTrigger::GraphEvent {
            event: "staleness_scan_complete".into(),
        },
        action: RoutineAction {
            skill_id: "urn:skill:stale-sweep".into(),
            version: "2.0.1".into(),
            parameters: serde_json::json!({ "threshold_days": 90 }),
        },
        output_target: OutputTarget::PodPath {
            path: "/private/workspaces/stale-report-weekly.jsonld".into(),
        },
        permissions: RoutinePermissions {
            tool_scopes: vec!["ontology_discover".into(), "graph_query".into()],
            data_scopes: vec![
                "pod:/private/kg/*".into(),
                "pod:/private/workspaces/*".into(),
            ],
        },
        delegated_cap_id: cap_id.into(),
        delegated_cap_expiry: now + ChronoDuration::hours(24),
        active: true,
        quiet_hours: None,
        max_runs_per_day: 1,
        consecutive_failure_suspend_threshold: 3,
        created_at: now,
        expires_at: now + ChronoDuration::days(183),
        schema_version: "1.0".into(),
    }
}

/// (c) Audit new broker cases — ontology-event-triggered, inbox-only.
pub fn example_broker_audit(owner_webid: &str, cap_id: &str) -> Routine {
    let now = Utc::now();
    Routine {
        routine_id: "rt-003".into(),
        owner_webid: owner_webid.into(),
        name: "Audit new broker cases".into(),
        description: "Brief the contributor when a broker case opens for them.".into(),
        trigger: RoutineTrigger::OntologyEvent {
            event: "BrokerCaseOpened".into(),
            filter: serde_json::json!({
                "category": "contributor_mesh_share",
                "subject_contributor": "self"
            }),
        },
        action: RoutineAction {
            skill_id: "urn:skill:case-audit-brief".into(),
            version: "0.9.2".into(),
            parameters: serde_json::json!({}),
        },
        output_target: OutputTarget::Inbox {
            path: "/inbox/broker-audit/".into(),
        },
        permissions: RoutinePermissions {
            tool_scopes: vec!["broker_case_read".into()],
            data_scopes: vec!["pod:/private/contributor-profile/share-log.jsonld".into()],
        },
        delegated_cap_id: cap_id.into(),
        delegated_cap_expiry: now + ChronoDuration::hours(24),
        active: true,
        quiet_hours: None,
        max_runs_per_day: 10,
        consecutive_failure_suspend_threshold: 3,
        created_at: now,
        expires_at: now + ChronoDuration::days(183),
        schema_version: "1.0".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_daily_cron() {
        let c = CronExpr::parse("0 8 * * *").unwrap();
        assert_eq!(c, CronExpr::Daily { hour: 8, minute: 0 });
    }

    #[test]
    fn parse_weekdays_cron() {
        let c = CronExpr::parse("0 8 * * 1-5").unwrap();
        match c {
            CronExpr::Weekly { hour, minute, weekdays } => {
                assert_eq!(hour, 8);
                assert_eq!(minute, 0);
                assert_eq!(weekdays.len(), 5);
                assert!(weekdays.contains(&Weekday::Mon));
                assert!(weekdays.contains(&Weekday::Fri));
                assert!(!weekdays.contains(&Weekday::Sat));
            }
            other => panic!("expected Weekly, got {:?}", other),
        }
    }

    #[test]
    fn parse_every_interval() {
        let c = CronExpr::parse("@every 30s").unwrap();
        assert_eq!(c, CronExpr::Every(ChronoDuration::seconds(30)));
    }

    #[test]
    fn daily_next_after_today_midnight() {
        let base = Utc.with_ymd_and_hms(2026, 4, 20, 0, 0, 0).single().unwrap();
        let nxt = CronExpr::Daily { hour: 8, minute: 0 }.next_after(base).unwrap();
        assert_eq!(nxt.hour(), 8);
        assert_eq!(nxt.day(), 20);
    }

    #[test]
    fn daily_next_wraps_to_tomorrow() {
        let base = Utc.with_ymd_and_hms(2026, 4, 20, 9, 0, 0).single().unwrap();
        let nxt = CronExpr::Daily { hour: 8, minute: 0 }.next_after(base).unwrap();
        assert_eq!(nxt.day(), 21);
        assert_eq!(nxt.hour(), 8);
    }

    #[test]
    fn invariants_envelope_check() {
        let mut r = example_research_brief("w", "cap-1");
        r.delegated_cap_expiry = r.expires_at + ChronoDuration::hours(1);
        let err = r
            .validate_invariants(&["pod:/private/*".into()])
            .unwrap_err();
        assert!(matches!(err, RoutineError::CapExpiryOutOfEnvelope { .. }));
    }

    #[test]
    fn data_scopes_subset_check() {
        let r = example_research_brief("w", "cap-1");
        // cap missing agent-memory scope → should reject
        let err = r
            .validate_invariants(&["pod:/private/kg/*".into()])
            .unwrap_err();
        assert_eq!(err, RoutineError::DataScopeNotSubset);
    }

    #[test]
    fn output_mismatch_check() {
        let mut r = example_research_brief("w", "cap-1");
        r.output_target = OutputTarget::Inbox {
            path: "/private/wrong/".into(),
        };
        let err = r
            .validate_invariants(&[
                "pod:/private/*".into(),
                "pod:/private/agent-memory/*".into(),
            ])
            .unwrap_err();
        assert!(matches!(err, RoutineError::OutputMismatch(_)));
    }

    #[test]
    fn quiet_hours_wraparound() {
        let r = example_research_brief("w", "cap-1");
        let t = Utc.with_ymd_and_hms(2026, 4, 20, 23, 0, 0).single().unwrap();
        assert!(r.is_quiet(t));
        let t = Utc.with_ymd_and_hms(2026, 4, 20, 12, 0, 0).single().unwrap();
        assert!(!r.is_quiet(t));
        let t = Utc.with_ymd_and_hms(2026, 4, 20, 5, 0, 0).single().unwrap();
        assert!(r.is_quiet(t));
    }

    #[test]
    fn inbox_ns_extracted() {
        let t = OutputTarget::Inbox {
            path: "/inbox/research-brief/".into(),
        };
        assert_eq!(t.inbox_ns(), Some("research-brief"));
    }

    #[test]
    fn example_routines_validate() {
        let caps = vec![
            "pod:/private/*".into(),
            "pod:/private/agent-memory/*".into(),
            "pod:/private/contributor-profile/share-log.jsonld".into(),
        ];
        assert!(example_research_brief("w", "c").validate_invariants(&caps).is_ok());
        assert!(example_stale_sweep("w", "c").validate_invariants(&caps).is_ok());
        assert!(example_broker_audit("w", "c").validate_invariants(&caps).is_ok());
    }
}
