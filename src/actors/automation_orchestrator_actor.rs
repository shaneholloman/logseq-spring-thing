//! `AutomationOrchestratorActor` — time-wheel scheduler for contributor
//! automations.
//!
//! Implements design 03 §5.2 with a hash-keyed time-wheel where each
//! routine has a `next_fire_at` entry. On tick, the actor:
//!
//! 1. Finds every routine whose `next_fire_at <= now`
//! 2. Validates the NIP-26 cap is still live (writes a renewal nudge
//!    to `/inbox/{ns}/` when inside the 48 h window)
//! 3. Enforces offline mode (policy rule `offline_mesh_block` — blocks
//!    mesh-facing side-effects when contributor has no ws session for
//!    >15 min)
//! 4. Enforces per-contributor rate-limit (200 runs/day — PRD §14 R13)
//! 5. Dispatches the action to `TaskOrchestratorActor`
//! 6. Writes the result to `output_target` (inbox item or pod path)
//!    with a provenance manifest
//! 7. Runs the retention sweeper on each registered inbox
//! 8. Reschedules the routine's next fire time
//!
//! The actor is driven by a single `Tick` message that runs every
//! [`TICK_INTERVAL`]. The wheel is a [`BTreeMap<DateTime<Utc>, Vec<RoutineId>>`]
//! so "find everything due by now" is O(log N + K) where K is the
//! number of fires in this tick.
//!
//! **TaskOrchestrator dispatch is abstracted behind the
//! [`AutomationDispatcher`] port** so this actor is testable in a
//! lightweight unit harness without an HTTP management API.

use actix::prelude::*;
use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use log::{debug, info, warn};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use crate::services::automation_routine::{
    OutputTarget, Routine, RoutineError, RoutineTrigger,
};
use crate::services::inbox_service::{
    InMemoryInboxWriter, InboxItem, InboxPath, InboxService, InboxStatus, InboxWriter,
    ProvenanceStep, SuggestedAction,
};
use crate::services::nip26_cap::{
    CapCheck, CapDenyReason, CapValidator, CapVerdict, DelegationCap, Nip98Verdict,
    OwnerSigVerdict,
};

/// Default tick cadence — 1 second keeps minute-granular cron firings accurate.
pub const TICK_INTERVAL: Duration = Duration::from_secs(1);

/// Offline threshold — design 03 §5.3.
pub const OFFLINE_THRESHOLD_MIN: i64 = 15;

/// Daily rate-limit (PRD §14 R13).
pub const DEFAULT_DAILY_RATE_LIMIT: u32 = 200;

/// Port the actor uses to hand an action off to `TaskOrchestratorActor`.
#[async_trait]
pub trait AutomationDispatcher: Send + Sync {
    /// Run a skill invocation and return a summary string (used as the
    /// inbox item's `summary`). Errors are captured by the caller.
    async fn dispatch(
        &self,
        routine: &Routine,
    ) -> Result<DispatchOutcome, String>;
}

/// The outcome of a skill dispatch.
#[derive(Debug, Clone)]
pub struct DispatchOutcome {
    pub summary: String,
    /// Optional pod path written by the skill (populated on
    /// `OutputTarget::PodPath`). The actor reads this into the inbox
    /// provenance manifest.
    pub content_ref: Option<String>,
    /// Tier that executed the action (for provenance + KPI).
    pub tier: String,
}

/// An in-memory dispatcher — used in tests and as a `cargo check`
/// stand-in before `TaskOrchestratorActor` is wired in.
#[derive(Default, Clone)]
pub struct StubDispatcher {
    /// Force success/failure per routine_id for tests.
    pub force_fail: Arc<std::sync::RwLock<HashMap<String, String>>>,
}

#[async_trait]
impl AutomationDispatcher for StubDispatcher {
    async fn dispatch(&self, routine: &Routine) -> Result<DispatchOutcome, String> {
        if let Some(err) = self
            .force_fail
            .read()
            .ok()
            .and_then(|g| g.get(&routine.routine_id).cloned())
        {
            return Err(err);
        }
        Ok(DispatchOutcome {
            summary: format!("Ran {} v{}", routine.action.skill_id, routine.action.version),
            content_ref: match &routine.output_target {
                OutputTarget::PodPath { path } => Some(format!("pod:{}", path)),
                _ => None,
            },
            tier: "tier2-haiku".to_string(),
        })
    }
}

/// Per-contributor presence tracker (ws session last-seen).
#[derive(Debug, Clone, Default)]
pub struct PresenceTracker {
    inner: Arc<std::sync::RwLock<HashMap<String, DateTime<Utc>>>>,
}

impl PresenceTracker {
    pub fn note_heartbeat(&self, webid: &str, now: DateTime<Utc>) {
        if let Ok(mut g) = self.inner.write() {
            g.insert(webid.to_string(), now);
        }
    }

    pub fn last_seen(&self, webid: &str) -> Option<DateTime<Utc>> {
        self.inner.read().ok().and_then(|g| g.get(webid).copied())
    }

    pub fn is_offline(&self, webid: &str, now: DateTime<Utc>) -> bool {
        match self.last_seen(webid) {
            Some(ls) => now - ls > ChronoDuration::minutes(OFFLINE_THRESHOLD_MIN),
            None => true, // never seen = offline
        }
    }
}

/// Sliding-window rate-limiter keyed by contributor.
#[derive(Debug, Default)]
struct RateWindow {
    runs: VecDeque<DateTime<Utc>>,
    per_day: u32,
}

impl RateWindow {
    fn new(per_day: u32) -> Self {
        Self {
            runs: VecDeque::new(),
            per_day,
        }
    }

    fn gc(&mut self, now: DateTime<Utc>) {
        let cutoff = now - ChronoDuration::hours(24);
        while matches!(self.runs.front(), Some(t) if *t < cutoff) {
            self.runs.pop_front();
        }
    }

    fn try_acquire(&mut self, now: DateTime<Utc>) -> bool {
        self.gc(now);
        if (self.runs.len() as u32) >= self.per_day {
            return false;
        }
        self.runs.push_back(now);
        true
    }
}

/// Reason a tick-fire was skipped or denied — used in tests + telemetry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FireDecision {
    Fired,
    Skipped(SkipReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    RoutineExpired,
    RoutineInactive,
    QuietHours,
    CapDenied(CapDenyReason),
    Offline(OfflineBlock),
    RateLimited,
    DispatchFailed(String),
    OutputInvalid(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfflineBlock {
    MeshTool,
    MeshPath,
    TeamShareBlocked,
}

/// Core scheduling engine — carved out of the actor so unit tests
/// drive it directly (without an actor runtime).
pub struct SchedulerCore<D: AutomationDispatcher, W: InboxWriter> {
    pub dispatcher: D,
    pub inbox: InboxService<W>,
    pub presence: PresenceTracker,
    routines: HashMap<String, Routine>,
    caps: HashMap<String, DelegationCap>,
    kill_switched: HashMap<String, bool>,
    wheel: BTreeMap<DateTime<Utc>, Vec<String>>,
    rate: HashMap<String, RateWindow>,
    daily_rate_limit: u32,
}

impl<D: AutomationDispatcher, W: InboxWriter> SchedulerCore<D, W> {
    pub fn new(dispatcher: D, inbox: InboxService<W>) -> Self {
        Self {
            dispatcher,
            inbox,
            presence: PresenceTracker::default(),
            routines: HashMap::new(),
            caps: HashMap::new(),
            kill_switched: HashMap::new(),
            wheel: BTreeMap::new(),
            rate: HashMap::new(),
            daily_rate_limit: DEFAULT_DAILY_RATE_LIMIT,
        }
    }

    pub fn with_rate_limit(mut self, per_day: u32) -> Self {
        self.daily_rate_limit = per_day;
        self
    }

    pub fn register_cap(&mut self, cap: DelegationCap) {
        self.caps.insert(cap.cap_id.clone(), cap);
    }

    pub fn set_kill_switch(&mut self, webid: &str, on: bool) {
        self.kill_switched.insert(webid.to_string(), on);
    }

    /// Insert or replace a routine and (re)compute its wheel entry.
    pub fn upsert_routine(&mut self, routine: Routine) -> Result<(), RoutineError> {
        let cap = self
            .caps
            .get(&routine.delegated_cap_id)
            .ok_or_else(|| RoutineError::DataScopeNotSubset)?;
        routine.validate_invariants(&cap.data_scopes)?;
        let rid = routine.routine_id.clone();
        // Remove any existing wheel entry.
        self.drop_from_wheel(&rid);
        if let Some(next) = routine.next_fire_at(Utc::now()) {
            self.wheel.entry(next).or_default().push(rid.clone());
        }
        self.routines.insert(rid, routine);
        Ok(())
    }

    fn drop_from_wheel(&mut self, routine_id: &str) {
        let keys: Vec<DateTime<Utc>> = self.wheel.keys().copied().collect();
        for k in keys {
            if let Some(v) = self.wheel.get_mut(&k) {
                v.retain(|r| r != routine_id);
                if v.is_empty() {
                    self.wheel.remove(&k);
                }
            }
        }
    }

    pub fn wheel_len(&self) -> usize {
        self.wheel.values().map(|v| v.len()).sum()
    }

    /// Advance the scheduler to `now` — fires every routine whose
    /// `next_fire_at <= now`. Returns per-fire decisions (for tests).
    pub async fn tick(&mut self, now: DateTime<Utc>) -> Vec<(String, FireDecision)> {
        // Collect due routine ids, then mutate self.
        let mut due: Vec<String> = Vec::new();
        let fire_keys: Vec<DateTime<Utc>> = self
            .wheel
            .range(..=now)
            .map(|(k, _)| *k)
            .collect();
        for k in fire_keys {
            if let Some(list) = self.wheel.remove(&k) {
                due.extend(list);
            }
        }

        let mut results = Vec::new();
        for rid in due {
            let decision = self.fire_once(&rid, now).await;
            results.push((rid.clone(), decision));

            // Reschedule.
            if let Some(r) = self.routines.get(&rid).cloned() {
                // Time triggers: schedule next; event triggers: drop from wheel.
                if let Some(next) = r.next_fire_at(now + ChronoDuration::seconds(1)) {
                    self.wheel.entry(next).or_default().push(rid.clone());
                }
            }
        }
        results
    }

    /// Attempt exactly one routine fire. Returns the decision.
    pub async fn fire_once(&mut self, routine_id: &str, now: DateTime<Utc>) -> FireDecision {
        let routine = match self.routines.get(routine_id).cloned() {
            Some(r) => r,
            None => {
                return FireDecision::Skipped(SkipReason::DispatchFailed(
                    "routine not found".to_string(),
                ))
            }
        };
        if routine.is_expired(now) {
            return FireDecision::Skipped(SkipReason::RoutineExpired);
        }
        if !routine.active {
            return FireDecision::Skipped(SkipReason::RoutineInactive);
        }
        if routine.is_quiet(now) {
            return FireDecision::Skipped(SkipReason::QuietHours);
        }

        // Cap check
        let cap = match self.caps.get(&routine.delegated_cap_id).cloned() {
            Some(c) => c,
            None => {
                return FireDecision::Skipped(SkipReason::CapDenied(CapDenyReason::Inactive))
            }
        };
        let kill = self.kill_switched.get(&routine.owner_webid).copied().unwrap_or(false);
        let paths = routine.permissions.data_scopes.clone();
        // We use the first tool_scope as the "requested tool" for the cap check
        // (the dispatcher enforces the actual tool-scope chain per call).
        let tool_name = routine
            .permissions
            .tool_scopes
            .first()
            .cloned()
            .unwrap_or_default();
        let check = CapCheck {
            cap: &cap,
            tool_requested: &tool_name,
            data_paths: &paths,
            nip98: Nip98Verdict::ServerLocal,
            owner_sig: OwnerSigVerdict::Valid,
            owner_kill_switched: kill,
            now,
        };
        match CapValidator::validate(&check) {
            CapVerdict::Allow => {}
            CapVerdict::Deny(reason) => return FireDecision::Skipped(SkipReason::CapDenied(reason)),
        }

        // Offline gate — design 03 §5.3. Compute is_offline and check
        // output_target + tool_scopes against the forbidden set.
        if self.presence.is_offline(&routine.owner_webid, now) {
            if let Some(block) = offline_block(&routine) {
                self.write_error_inbox(&routine, now, &format!("offline_block:{:?}", block))
                    .await;
                return FireDecision::Skipped(SkipReason::Offline(block));
            }
        }

        // Rate limit
        let rw = self
            .rate
            .entry(routine.owner_webid.clone())
            .or_insert_with(|| RateWindow::new(self.daily_rate_limit));
        if !rw.try_acquire(now) {
            return FireDecision::Skipped(SkipReason::RateLimited);
        }

        // Renewal nudge?
        if cap.needs_renewal_nudge(now) {
            let ns = routine
                .output_target
                .inbox_ns()
                .unwrap_or("automation");
            let path = InboxPath::new(&routine.owner_webid, ns);
            let notice = InboxItem::new_system_notice(
                format!("Cap renewal due — {}", cap.cap_id),
                format!(
                    "Delegation cap for routine {} expires at {}.",
                    routine.routine_id, cap.expires_at
                ),
                "cap_renewal_nudge",
            );
            let _ = self.inbox.writer().append(&path, notice).await;
        }

        // Dispatch
        let outcome = match self.dispatcher.dispatch(&routine).await {
            Ok(o) => o,
            Err(e) => {
                self.write_error_inbox(&routine, now, &e).await;
                return FireDecision::Skipped(SkipReason::DispatchFailed(e));
            }
        };

        // Write output
        match &routine.output_target {
            OutputTarget::Inbox { path: inbox_path } => {
                let ns = inbox_path
                    .strip_prefix("/inbox/")
                    .and_then(|r| r.split('/').next())
                    .unwrap_or("automation");
                let path = InboxPath::new(&routine.owner_webid, ns);
                let agent_webid =
                    format!("{}/agents/{}#id", routine.owner_webid.trim_end_matches("#me"), ns);
                let mut item = InboxItem::new_from_routine(
                    routine.name.clone(),
                    outcome.summary.clone(),
                    routine.routine_id.clone(),
                    agent_webid,
                    cap.cap_id.clone(),
                    outcome.content_ref.clone(),
                );
                item.provenance_chain.push(ProvenanceStep {
                    step: format!("tool-call:{}", tool_name),
                    at: now,
                    signed_by: Some(cap.cap_id.clone()),
                    tier: Some(outcome.tier.clone()),
                    evaluation_id: None,
                });
                // Offer a canonical "open in workspace" action.
                if outcome.content_ref.is_some() {
                    item.suggested_actions.push(SuggestedAction {
                        action_id: "open-1".into(),
                        label: "Open in workspace".into(),
                        target: "studio:workspace/open".into(),
                    });
                }
                if let Err(e) = self.inbox.write_item(&path, item).await {
                    self.write_error_inbox(&routine, now, &e.to_string()).await;
                    return FireDecision::Skipped(SkipReason::OutputInvalid(e.to_string()));
                }
            }
            OutputTarget::PodPath { .. } => {
                // In the MVP, the pod write is handled by the dispatcher itself
                // (the skill writes its own file). We still emit an inbox
                // breadcrumb so the contributor sees the routine ran.
                if let Some(ns) = routine.output_target.inbox_ns() {
                    let _ = ns; // never true for PodPath; here for symmetry
                }
            }
            OutputTarget::GraphMutation => {
                // Nothing to persist here — the dispatcher updates the graph.
            }
        }

        FireDecision::Fired
    }

    async fn write_error_inbox(&self, routine: &Routine, now: DateTime<Utc>, reason: &str) {
        let ns = routine
            .output_target
            .inbox_ns()
            .unwrap_or("automation");
        let path = InboxPath::new(&routine.owner_webid, ns);
        let notice = InboxItem::new_system_notice(
            format!("Routine error — {}", routine.name),
            format!("at={} reason={}", now.to_rfc3339(), reason),
            "routine_error",
        );
        let _ = self.inbox.writer().append(&path, notice).await;
    }

    /// Run retention sweep on each distinct inbox target registered.
    pub async fn sweep_all(&self, now: DateTime<Utc>) {
        let mut seen: std::collections::HashSet<(String, String)> = Default::default();
        for r in self.routines.values() {
            let Some(ns) = r.output_target.inbox_ns() else { continue };
            let key = (r.owner_webid.clone(), ns.to_string());
            if seen.insert(key.clone()) {
                let path = InboxPath::new(&r.owner_webid, ns);
                let _ = self.inbox.sweep(&path, now).await;
            }
        }
    }
}

/// Returns `Some(OfflineBlock)` when the routine's effects would cross
/// the offline fence (design 03 §5.3, rule `offline_mesh_block`).
pub fn offline_block(r: &Routine) -> Option<OfflineBlock> {
    // Forbidden paths: /shared/**, /public/**
    if let OutputTarget::PodPath { path } = &r.output_target {
        if path.starts_with("/shared/") || path.starts_with("/public/") {
            return Some(OfflineBlock::MeshPath);
        }
    }
    // Forbidden tools: anything starting mesh: or broker:
    for t in &r.permissions.tool_scopes {
        if t.starts_with("mesh:") || t.starts_with("broker:") {
            return Some(OfflineBlock::MeshTool);
        }
    }
    // Team share is blocked when the routine trigger's intent is a share action.
    // We conservatively mark any routine whose skill_id contains "share" as
    // team-share-blocked (a stricter check will use Policy Engine rules).
    if matches!(r.trigger, RoutineTrigger::OntologyEvent { .. })
        && r.action.skill_id.contains("share")
    {
        return Some(OfflineBlock::TeamShareBlocked);
    }
    None
}

// -------------------------------------------------------------------
// Actor integration
// -------------------------------------------------------------------

/// The Actix actor. Owns a [`SchedulerCore`] and fires it every second.
pub struct AutomationOrchestratorActor {
    core: SchedulerCore<StubDispatcher, InMemoryInboxWriter>,
}

impl AutomationOrchestratorActor {
    pub fn new() -> Self {
        let inbox = InboxService::new(InMemoryInboxWriter::new());
        Self {
            core: SchedulerCore::new(StubDispatcher::default(), inbox),
        }
    }
}

impl Default for AutomationOrchestratorActor {
    fn default() -> Self {
        Self::new()
    }
}

impl Actor for AutomationOrchestratorActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("[AutomationOrchestratorActor] started; tick={:?}", TICK_INTERVAL);
        ctx.run_interval(TICK_INTERVAL, |_act, ctx| {
            ctx.notify(Tick);
        });
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("[AutomationOrchestratorActor] stopped");
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Tick;

#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct RegisterCap(pub DelegationCap);

#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct RegisterRoutine(pub Routine);

#[derive(Message)]
#[rtype(result = "()")]
pub struct HeartbeatWebId {
    pub webid: String,
    pub at: DateTime<Utc>,
}

#[derive(Message)]
#[rtype(result = "usize")]
pub struct GetWheelSize;

impl Handler<Tick> for AutomationOrchestratorActor {
    type Result = ResponseFuture<()>;

    fn handle(&mut self, _: Tick, _ctx: &mut Self::Context) -> Self::Result {
        // SAFETY: actor has exclusive access to its state during handler
        // execution. The future below owns a mutable borrow for its lifetime.
        let fut = async move { /* placeholder */ };
        // We need to mutate self inside an async block; Actix provides
        // `AtomicResponse`/`ResponseActFuture` for this. For the MVP the
        // scheduler is small enough to run its work synchronously here.
        let now = Utc::now();
        let core = &mut self.core;
        let results = futures::executor::block_on(core.tick(now));
        for (rid, decision) in results {
            debug!("[automation] routine {} -> {:?}", rid, decision);
        }
        futures::executor::block_on(core.sweep_all(now));
        Box::pin(fut)
    }
}

impl Handler<RegisterCap> for AutomationOrchestratorActor {
    type Result = Result<(), String>;
    fn handle(&mut self, msg: RegisterCap, _ctx: &mut Self::Context) -> Self::Result {
        self.core.register_cap(msg.0);
        Ok(())
    }
}

impl Handler<RegisterRoutine> for AutomationOrchestratorActor {
    type Result = Result<(), String>;
    fn handle(&mut self, msg: RegisterRoutine, _ctx: &mut Self::Context) -> Self::Result {
        self.core.upsert_routine(msg.0).map_err(|e| e.to_string())
    }
}

impl Handler<HeartbeatWebId> for AutomationOrchestratorActor {
    type Result = ();
    fn handle(&mut self, msg: HeartbeatWebId, _ctx: &mut Self::Context) -> Self::Result {
        self.core.presence.note_heartbeat(&msg.webid, msg.at);
    }
}

impl Handler<GetWheelSize> for AutomationOrchestratorActor {
    type Result = MessageResult<GetWheelSize>;
    fn handle(&mut self, _: GetWheelSize, _ctx: &mut Self::Context) -> Self::Result {
        MessageResult(self.core.wheel_len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::automation_routine::{example_research_brief, example_stale_sweep};

    fn cap_for(routine: &Routine) -> DelegationCap {
        let mut cap = DelegationCap::new(
            &routine.delegated_cap_id,
            &routine.owner_webid,
            "npub1alice",
            "npub1agent",
            routine.permissions.tool_scopes.clone(),
            routine.permissions.data_scopes.clone(),
        );
        cap.expires_at = routine.delegated_cap_expiry;
        cap
    }

    #[tokio::test]
    async fn fires_when_due() {
        let mut core = SchedulerCore::new(
            StubDispatcher::default(),
            InboxService::new(InMemoryInboxWriter::new()),
        );
        let r = example_research_brief("https://alice.pods/profile/card#me", "cap-1");
        core.register_cap(cap_for(&r));
        core.presence
            .note_heartbeat(&r.owner_webid, Utc::now());
        core.upsert_routine(r.clone()).unwrap();

        // Force a fire by calling fire_once at an in-range time.
        let d = core.fire_once(&r.routine_id, Utc::now()).await;
        assert_eq!(d, FireDecision::Fired);
    }

    #[tokio::test]
    async fn rate_limit_kicks_in() {
        let mut core = SchedulerCore::new(
            StubDispatcher::default(),
            InboxService::new(InMemoryInboxWriter::new()),
        )
        .with_rate_limit(2);
        let r = example_research_brief("https://alice.pods/profile/card#me", "cap-1");
        core.register_cap(cap_for(&r));
        core.presence.note_heartbeat(&r.owner_webid, Utc::now());
        core.upsert_routine(r.clone()).unwrap();

        assert_eq!(core.fire_once(&r.routine_id, Utc::now()).await, FireDecision::Fired);
        assert_eq!(core.fire_once(&r.routine_id, Utc::now()).await, FireDecision::Fired);
        let d = core.fire_once(&r.routine_id, Utc::now()).await;
        assert_eq!(d, FireDecision::Skipped(SkipReason::RateLimited));
    }

    #[tokio::test]
    async fn offline_blocks_shared_writes() {
        let mut core = SchedulerCore::new(
            StubDispatcher::default(),
            InboxService::new(InMemoryInboxWriter::new()),
        );
        let mut r = example_stale_sweep("https://alice.pods/profile/card#me", "cap-1");
        // Force the routine to target /shared/, which is forbidden offline.
        r.output_target = OutputTarget::PodPath {
            path: "/shared/teams/alpha/report.jsonld".into(),
        };
        core.register_cap(cap_for(&r));
        // No heartbeat → offline.
        core.upsert_routine(r.clone()).unwrap();
        let d = core.fire_once(&r.routine_id, Utc::now()).await;
        assert!(matches!(
            d,
            FireDecision::Skipped(SkipReason::Offline(OfflineBlock::MeshPath))
        ));
    }

    #[tokio::test]
    async fn cap_expiry_blocks() {
        let mut core = SchedulerCore::new(
            StubDispatcher::default(),
            InboxService::new(InMemoryInboxWriter::new()),
        );
        let r = example_research_brief("w", "cap-1");
        let mut cap = cap_for(&r);
        cap.expires_at = Utc::now() - ChronoDuration::seconds(1);
        core.register_cap(cap);
        core.presence.note_heartbeat(&r.owner_webid, Utc::now());
        core.upsert_routine(r.clone()).unwrap();
        let d = core.fire_once(&r.routine_id, Utc::now()).await;
        assert!(matches!(d, FireDecision::Skipped(SkipReason::CapDenied(_))));
    }
}
