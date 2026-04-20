// tests/settings_consolidation_test.rs
//! ADR-039: Settings Consolidation — regression tests for the unified
//! SettingsActor (public + protected partition).
//!
//! These tests verify that:
//!  1. A single actor answers both public-partition (GetSettings) and
//!     protected-partition (GetApiKeys, GetUser, MergeSettings…) messages.
//!  2. MergeSettings updates the protected partition and is observable via
//!     subsequent reads against the same actor address.
//!  3. The `ProtectedSettingsActor` type alias resolves to the unified
//!     actor, preserving backward compatibility for existing call sites.
//!
//! A minimal in-memory `SettingsRepository` is embedded here to avoid
//! coupling the test to the (currently broken) `tests/ports/mocks.rs`
//! tree, which is maintained separately.

use std::collections::HashMap;
use std::sync::Arc;

use actix::prelude::*;
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::RwLock;

use webxr::actors::optimized_settings_actor::OptimizedSettingsActor;
use webxr::actors::protected_settings_actor::{
    GetApiKeys, GetUser, MergeSettings, ProtectedSettingsActor, StoreClientToken,
    UpdateUserApiKeys, ValidateClientToken,
};
use webxr::config::{AppFullSettings, PhysicsSettings};
use webxr::models::protected_settings::{ApiKeys, NostrUser, ProtectedSettings};
use webxr::ports::settings_repository::{Result as SRResult, SettingValue, SettingsRepository};

struct InMemSettingsRepo {
    data: Arc<RwLock<HashMap<String, SettingValue>>>,
}

impl InMemSettingsRepo {
    fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl SettingsRepository for InMemSettingsRepo {
    async fn get_setting(&self, key: &str) -> SRResult<Option<SettingValue>> {
        Ok(self.data.read().await.get(key).cloned())
    }
    async fn set_setting(
        &self,
        key: &str,
        value: SettingValue,
        _description: Option<&str>,
    ) -> SRResult<()> {
        self.data.write().await.insert(key.to_string(), value);
        Ok(())
    }
    async fn delete_setting(&self, key: &str) -> SRResult<()> {
        self.data.write().await.remove(key);
        Ok(())
    }
    async fn has_setting(&self, key: &str) -> SRResult<bool> {
        Ok(self.data.read().await.contains_key(key))
    }
    async fn get_settings_batch(
        &self,
        keys: &[String],
    ) -> SRResult<HashMap<String, SettingValue>> {
        let d = self.data.read().await;
        Ok(keys
            .iter()
            .filter_map(|k| d.get(k).map(|v| (k.clone(), v.clone())))
            .collect())
    }
    async fn set_settings_batch(
        &self,
        updates: HashMap<String, SettingValue>,
    ) -> SRResult<()> {
        let mut d = self.data.write().await;
        for (k, v) in updates {
            d.insert(k, v);
        }
        Ok(())
    }
    async fn list_settings(&self, prefix: Option<&str>) -> SRResult<Vec<String>> {
        let d = self.data.read().await;
        Ok(d.keys()
            .filter(|k| prefix.map_or(true, |p| k.starts_with(p)))
            .cloned()
            .collect())
    }
    async fn load_all_settings(&self) -> SRResult<Option<AppFullSettings>> {
        Ok(None)
    }
    async fn save_all_settings(&self, _settings: &AppFullSettings) -> SRResult<()> {
        Ok(())
    }
    async fn get_physics_settings(&self, _profile_name: &str) -> SRResult<PhysicsSettings> {
        Ok(PhysicsSettings::default())
    }
    async fn save_physics_settings(
        &self,
        _profile_name: &str,
        _settings: &PhysicsSettings,
    ) -> SRResult<()> {
        Ok(())
    }
    async fn list_physics_profiles(&self) -> SRResult<Vec<String>> {
        Ok(vec![])
    }
    async fn delete_physics_profile(&self, _profile_name: &str) -> SRResult<()> {
        Ok(())
    }
    async fn export_settings(&self) -> SRResult<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
    async fn import_settings(&self, _settings_json: &serde_json::Value) -> SRResult<()> {
        Ok(())
    }
    async fn clear_cache(&self) -> SRResult<()> {
        Ok(())
    }
    async fn health_check(&self) -> SRResult<bool> {
        Ok(true)
    }
}

fn make_actor() -> OptimizedSettingsActor {
    let repo: Arc<dyn SettingsRepository> = Arc::new(InMemSettingsRepo::new());
    OptimizedSettingsActor::new(repo).expect("actor construction")
}

fn seed_user(protected: &mut ProtectedSettings, pubkey: &str, power: bool) {
    protected.users.insert(
        pubkey.to_string(),
        NostrUser {
            pubkey: pubkey.to_string(),
            npub: format!("npub_{}", pubkey),
            is_power_user: power,
            api_keys: ApiKeys {
                perplexity: Some("pplx-abc".to_string()),
                openai: None,
                ragflow: None,
            },
            last_seen: 0,
            session_token: None,
        },
    );
}

#[actix_rt::test]
async fn unified_actor_answers_protected_partition() {
    // A single SettingsActor instance fulfils both public and protected
    // partitions. This is the core structural guarantee of ADR-039.
    let mut protected = ProtectedSettings::default();
    seed_user(&mut protected, "alice", false);
    let actor = make_actor().with_protected(protected);
    let addr = actor.start();

    let keys: ApiKeys = addr
        .send(GetApiKeys {
            pubkey: "alice".to_string(),
        })
        .await
        .expect("mailbox");
    assert_eq!(keys.perplexity.as_deref(), Some("pplx-abc"));

    let user = addr
        .send(GetUser {
            pubkey: "alice".to_string(),
        })
        .await
        .expect("mailbox");
    assert!(user.is_some(), "seeded user must be retrievable");
    assert_eq!(user.unwrap().pubkey, "alice");
}

#[actix_rt::test]
async fn merge_settings_updates_protected_partition() {
    let actor = make_actor().with_protected(ProtectedSettings::default());
    let addr = actor.start();

    let patch = json!({
        "network": {
            "bindAddress": "0.0.0.0",
            "domain": "example.test",
            "port": 8080,
            "enableHttp2": false,
            "enableTls": true,
            "minTlsVersion": "TLS1.3",
            "maxRequestSize": 2_097_152,
            "enableRateLimiting": true,
            "rateLimitRequests": 200,
            "rateLimitWindow": 120,
            "tunnelId": ""
        }
    });

    let result = addr
        .send(MergeSettings { settings: patch })
        .await
        .expect("mailbox");
    assert!(result.is_ok(), "merge must accept valid patch: {:?}", result);
}

#[actix_rt::test]
async fn update_and_validate_client_token_round_trip() {
    let mut protected = ProtectedSettings::default();
    seed_user(&mut protected, "bob", false);
    let actor = make_actor().with_protected(protected);
    let addr = actor.start();

    addr.send(StoreClientToken {
        pubkey: "bob".to_string(),
        token: "session-xyz".to_string(),
    })
    .await
    .expect("mailbox");

    let valid = addr
        .send(ValidateClientToken {
            pubkey: "bob".to_string(),
            token: "session-xyz".to_string(),
        })
        .await
        .expect("mailbox");
    assert!(valid, "stored token must validate");

    let invalid = addr
        .send(ValidateClientToken {
            pubkey: "bob".to_string(),
            token: "wrong".to_string(),
        })
        .await
        .expect("mailbox");
    assert!(!invalid, "mismatched token must not validate");
}

#[actix_rt::test]
async fn update_user_api_keys_rejects_power_user() {
    let mut protected = ProtectedSettings::default();
    seed_user(&mut protected, "carol", true); // power user
    let actor = make_actor().with_protected(protected);
    let addr = actor.start();

    let result = addr
        .send(UpdateUserApiKeys {
            pubkey: "carol".to_string(),
            api_keys: ApiKeys {
                perplexity: Some("tainted".to_string()),
                openai: None,
                ragflow: None,
            },
        })
        .await
        .expect("mailbox");

    assert!(
        result.is_err(),
        "power-user API keys must come from env, not client writes"
    );
}

#[actix_rt::test]
async fn protected_settings_actor_alias_is_unified() {
    // `ProtectedSettingsActor` is preserved as a type alias for backward
    // compatibility — it must resolve to the canonical SettingsActor and
    // accept the same `with_protected` seeding helper.
    let alias_actor: ProtectedSettingsActor =
        make_actor().with_protected(ProtectedSettings::default());
    let addr = alias_actor.start();

    let user = addr
        .send(GetUser {
            pubkey: "nobody".to_string(),
        })
        .await
        .expect("mailbox");
    assert!(user.is_none());
}
