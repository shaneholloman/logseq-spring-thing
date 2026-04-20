// ADR-039 (Implemented 2026-04-20): ProtectedSettingsActor has been merged
// into the canonical SettingsActor (see optimized_settings_actor.rs).
//
// This module now only hosts the protected-partition *message types* so that
// existing call sites importing `crate::actors::protected_settings_actor::*`
// keep working. The actor type itself is a backward-compatible alias over
// `OptimizedSettingsActor` (a.k.a. `SettingsActor`).

use actix::prelude::*;
use serde_json::Value;

use crate::models::protected_settings::{ApiKeys, NostrUser};

/// Backward-compatible alias. All new code should use
/// [`crate::actors::SettingsActor`]. Existing callers referring to
/// `ProtectedSettingsActor` now resolve to the unified actor and therefore
/// address the same actor instance that serves public settings.
pub type ProtectedSettingsActor = crate::actors::optimized_settings_actor::OptimizedSettingsActor;

// ---------------------------------------------------------------------------
// Protected-partition messages. Handlers live on `OptimizedSettingsActor`
// (optimized_settings_actor.rs). Do not move them back; the unified actor is
// the single authority for protected settings after ADR-039.
// ---------------------------------------------------------------------------

#[derive(Message)]
#[rtype(result = "ApiKeys")]
pub struct GetApiKeys {
    pub pubkey: String,
}

#[derive(Message)]
#[rtype(result = "bool")]
pub struct ValidateClientToken {
    pub pubkey: String,
    pub token: String,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct StoreClientToken {
    pub pubkey: String,
    pub token: String,
}

#[derive(Message)]
#[rtype(result = "Result<NostrUser, String>")]
pub struct UpdateUserApiKeys {
    pub pubkey: String,
    pub api_keys: ApiKeys,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct CleanupExpiredTokens {
    pub max_age_hours: i64,
}

#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct MergeSettings {
    pub settings: Value,
}

#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct SaveSettings {
    pub path: String,
}

#[derive(Message)]
#[rtype(result = "Option<NostrUser>")]
pub struct GetUser {
    pub pubkey: String,
}
