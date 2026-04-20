//! NIP-26 delegated capability validation and renewal (per ADR-040 + design 03 §8.3).
//!
//! A `DelegationCap` binds a delegator (contributor WebID / npub) to a
//! delegatee pubkey with `tool_scopes`, `data_scopes`, and a TTL. Every
//! Pod or tool call performed under a cap MUST pass the six checks in
//! [`CapValidator::validate`]:
//!
//! 1. `active=true` AND now < `expires_at`
//! 2. Requested tool ∈ `tool_scopes`
//! 3. Every path the action touches is prefix-matched by at least one
//!    `data_scopes` glob (`*` suffix allowed)
//! 4. NIP-98 request-sig valid (deferred to the HTTP layer — caller
//!    tells us via [`Nip98Verdict`])
//! 5. `owner_sig` valid (deferred — caller tells us via
//!    [`OwnerSigVerdict`])
//! 6. Owner is not kill-switched (caller tells us)
//!
//! Validation is **pure** so the same code path runs in the
//! `AutomationOrchestratorActor`, the `/api/inbox` handler, and the
//! policy engine.
//!
//! Renewal is surfaced as a notification written to `/inbox/{agent-ns}/`
//! at `expires_at - RENEW_WINDOW`.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};

/// Default TTL for a freshly issued cap (design 03 §8.3 — 24 h).
pub const DEFAULT_CAP_TTL_HOURS: i64 = 24;

/// Renew-nudge window — write an inbox notification this far ahead of
/// `expires_at` (design 03 §8.3 — 48 h before expiry).
pub const RENEW_WINDOW_HOURS: i64 = 48;

/// Delegation-cap document stored under
/// `/private/contributor-profile/caps/{cap-id}.jsonld`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DelegationCap {
    #[serde(rename = "@id")]
    pub cap_id: String,
    pub delegator_webid: String,
    pub delegator_npub: String,
    pub delegatee_pubkey: String,
    pub tool_scopes: Vec<String>,
    pub data_scopes: Vec<String>,
    pub ttl_hours: i64,
    pub granted_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub owner_sig: String,
    pub active: bool,
    pub revocation_note: Option<String>,
}

impl DelegationCap {
    /// Build a cap with the default 24 h TTL and `active=true`.
    pub fn new(
        cap_id: impl Into<String>,
        delegator_webid: impl Into<String>,
        delegator_npub: impl Into<String>,
        delegatee_pubkey: impl Into<String>,
        tool_scopes: Vec<String>,
        data_scopes: Vec<String>,
    ) -> Self {
        let granted_at = Utc::now();
        let expires_at = granted_at + ChronoDuration::hours(DEFAULT_CAP_TTL_HOURS);
        Self {
            cap_id: cap_id.into(),
            delegator_webid: delegator_webid.into(),
            delegator_npub: delegator_npub.into(),
            delegatee_pubkey: delegatee_pubkey.into(),
            tool_scopes,
            data_scopes,
            ttl_hours: DEFAULT_CAP_TTL_HOURS,
            granted_at,
            expires_at,
            owner_sig: String::new(), // filled by NIP-07 signing flow
            active: true,
            revocation_note: None,
        }
    }

    /// Construct a renewed cap — new `cap_id`, fresh timestamps,
    /// identical scope. Widening requires a full re-issue (spec §8.3).
    pub fn renew(&self, new_cap_id: impl Into<String>) -> Self {
        let granted_at = Utc::now();
        let expires_at = granted_at + ChronoDuration::hours(self.ttl_hours);
        Self {
            cap_id: new_cap_id.into(),
            granted_at,
            expires_at,
            owner_sig: String::new(),
            active: true,
            revocation_note: None,
            ..self.clone()
        }
    }

    /// True when `now` is inside the renewal nudge window.
    pub fn needs_renewal_nudge(&self, now: DateTime<Utc>) -> bool {
        self.active
            && now < self.expires_at
            && (self.expires_at - now) <= ChronoDuration::hours(RENEW_WINDOW_HOURS)
    }

    /// True when the cap is still valid right now (checks 1 only).
    pub fn is_live(&self, now: DateTime<Utc>) -> bool {
        self.active && now < self.expires_at
    }
}

/// Outcome of the six-step cap validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapVerdict {
    Allow,
    Deny(CapDenyReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapDenyReason {
    Inactive,
    Expired,
    ToolOutOfScope(String),
    DataOutOfScope(String),
    Nip98Invalid,
    OwnerSigInvalid,
    OwnerKillSwitched,
}

/// NIP-98 sig verdict supplied by the HTTP layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nip98Verdict {
    /// The caller verified the NIP-98 signature on the request body.
    Valid,
    /// Not checked (server-internal call); treated as Valid for same-process use.
    ServerLocal,
    Invalid,
}

/// Owner-sig verdict supplied by the pod-read layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerSigVerdict {
    Valid,
    Invalid,
}

/// Context for a single cap check.
pub struct CapCheck<'a> {
    pub cap: &'a DelegationCap,
    pub tool_requested: &'a str,
    /// Every pod path the action will touch.
    pub data_paths: &'a [String],
    pub nip98: Nip98Verdict,
    pub owner_sig: OwnerSigVerdict,
    /// True if the owner's kill-switch is currently active.
    pub owner_kill_switched: bool,
    pub now: DateTime<Utc>,
}

/// Pure cap validator — no I/O, no clock, all side-effects lifted to
/// the caller.
pub struct CapValidator;

impl CapValidator {
    pub fn validate(check: &CapCheck<'_>) -> CapVerdict {
        // 1a — active flag
        if !check.cap.active {
            return CapVerdict::Deny(CapDenyReason::Inactive);
        }
        // 1b — TTL
        if check.now >= check.cap.expires_at {
            return CapVerdict::Deny(CapDenyReason::Expired);
        }
        // 2 — tool scope
        if !check
            .cap
            .tool_scopes
            .iter()
            .any(|s| s == check.tool_requested)
        {
            return CapVerdict::Deny(CapDenyReason::ToolOutOfScope(
                check.tool_requested.to_string(),
            ));
        }
        // 3 — data scope
        for path in check.data_paths {
            if !path_in_scopes(path, &check.cap.data_scopes) {
                return CapVerdict::Deny(CapDenyReason::DataOutOfScope(path.clone()));
            }
        }
        // 4 — NIP-98
        if matches!(check.nip98, Nip98Verdict::Invalid) {
            return CapVerdict::Deny(CapDenyReason::Nip98Invalid);
        }
        // 5 — owner sig
        if matches!(check.owner_sig, OwnerSigVerdict::Invalid) {
            return CapVerdict::Deny(CapDenyReason::OwnerSigInvalid);
        }
        // 6 — kill-switch
        if check.owner_kill_switched {
            return CapVerdict::Deny(CapDenyReason::OwnerKillSwitched);
        }
        CapVerdict::Allow
    }
}

/// Prefix-match `path` against any `glob` in `scopes`. Globs end in
/// `*` to mean "any suffix"; exact matches also allowed.
pub fn path_in_scopes(path: &str, scopes: &[String]) -> bool {
    scopes.iter().any(|glob| path_matches(path, glob))
}

fn path_matches(path: &str, glob: &str) -> bool {
    if let Some(stripped) = glob.strip_suffix('*') {
        path.starts_with(stripped)
    } else {
        path == glob
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_cap() -> DelegationCap {
        DelegationCap::new(
            "urn:nip26:cap:alice:sensei:2026-04-20",
            "https://alice.pods/profile/card#me",
            "npub1alice",
            "npub1sensei",
            vec!["sensei_nudge".into(), "ontology_discover".into()],
            vec![
                "pod:/inbox/sensei/*".into(),
                "pod:/private/agent-memory/sensei/*".into(),
            ],
        )
    }

    fn ok_check<'a>(cap: &'a DelegationCap, tool: &'a str, paths: &'a [String]) -> CapCheck<'a> {
        CapCheck {
            cap,
            tool_requested: tool,
            data_paths: paths,
            nip98: Nip98Verdict::ServerLocal,
            owner_sig: OwnerSigVerdict::Valid,
            owner_kill_switched: false,
            now: Utc::now(),
        }
    }

    #[test]
    fn allow_in_scope() {
        let cap = base_cap();
        let paths = vec!["pod:/inbox/sensei/item-1.jsonld".to_string()];
        let v = CapValidator::validate(&ok_check(&cap, "sensei_nudge", &paths));
        assert_eq!(v, CapVerdict::Allow);
    }

    #[test]
    fn deny_tool_out_of_scope() {
        let cap = base_cap();
        let paths = vec!["pod:/inbox/sensei/item-1.jsonld".to_string()];
        let v = CapValidator::validate(&ok_check(&cap, "broker_decide", &paths));
        assert!(matches!(v, CapVerdict::Deny(CapDenyReason::ToolOutOfScope(_))));
    }

    #[test]
    fn deny_data_out_of_scope() {
        let cap = base_cap();
        let paths = vec!["pod:/public/skills/x.md".to_string()];
        let v = CapValidator::validate(&ok_check(&cap, "sensei_nudge", &paths));
        assert!(matches!(v, CapVerdict::Deny(CapDenyReason::DataOutOfScope(_))));
    }

    #[test]
    fn deny_expired() {
        let mut cap = base_cap();
        cap.expires_at = Utc::now() - ChronoDuration::hours(1);
        let paths = vec!["pod:/inbox/sensei/i.jsonld".to_string()];
        let v = CapValidator::validate(&ok_check(&cap, "sensei_nudge", &paths));
        assert_eq!(v, CapVerdict::Deny(CapDenyReason::Expired));
    }

    #[test]
    fn deny_kill_switched() {
        let cap = base_cap();
        let paths = vec!["pod:/inbox/sensei/i.jsonld".to_string()];
        let mut check = ok_check(&cap, "sensei_nudge", &paths);
        check.owner_kill_switched = true;
        let v = CapValidator::validate(&check);
        assert_eq!(v, CapVerdict::Deny(CapDenyReason::OwnerKillSwitched));
    }

    #[test]
    fn renew_creates_fresh_cap() {
        let cap = base_cap();
        let renewed = cap.renew("urn:nip26:cap:alice:sensei:2026-04-21");
        assert_ne!(renewed.cap_id, cap.cap_id);
        assert_eq!(renewed.tool_scopes, cap.tool_scopes);
        assert!(renewed.expires_at > cap.expires_at - ChronoDuration::seconds(10));
        assert!(renewed.active);
    }

    #[test]
    fn renewal_nudge_within_48h() {
        let mut cap = base_cap();
        cap.expires_at = Utc::now() + ChronoDuration::hours(47);
        assert!(cap.needs_renewal_nudge(Utc::now()));
        cap.expires_at = Utc::now() + ChronoDuration::hours(72);
        assert!(!cap.needs_renewal_nudge(Utc::now()));
    }
}
