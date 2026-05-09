use crate::config::feature_access::FeatureAccess;
use crate::models::protected_settings::{ApiKeys, NostrUser};
use crate::utils::nip98::{
    parse_auth_header, validate_nip98_token, Nip98ValidationError, Nip98ValidationResult,
};
use log::{debug, error, info, warn};
use nostr_sdk::{event::Error as EventError, prelude::*};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;
use crate::utils::time;
use crate::utils::json::to_json;

#[cfg(feature = "redis")]
use redis::{AsyncCommands, Client as RedisClient};

#[derive(Debug, Error)]
pub enum NostrError {
    #[error("Invalid event: {0}")]
    InvalidEvent(String),
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("User not found")]
    UserNotFound,
    #[error("Invalid token")]
    InvalidToken,
    #[error("Session expired")]
    SessionExpired,
    #[error("Power user operation not allowed")]
    PowerUserOperation,
    #[error("Nostr event error: {0}")]
    NostrError(#[from] EventError),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("NIP-98 validation error: {0}")]
    Nip98Error(String),
}

impl Serialize for NostrError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("NostrError", 2)?;
        match self {
            NostrError::InvalidEvent(msg) => {
                state.serialize_field("type", "InvalidEvent")?;
                state.serialize_field("message", msg)?;
            }
            NostrError::InvalidSignature => {
                state.serialize_field("type", "InvalidSignature")?;
                state.serialize_field("message", "Invalid signature")?;
            }
            NostrError::UserNotFound => {
                state.serialize_field("type", "UserNotFound")?;
                state.serialize_field("message", "User not found")?;
            }
            NostrError::InvalidToken => {
                state.serialize_field("type", "InvalidToken")?;
                state.serialize_field("message", "Invalid token")?;
            }
            NostrError::SessionExpired => {
                state.serialize_field("type", "SessionExpired")?;
                state.serialize_field("message", "Session expired")?;
            }
            NostrError::PowerUserOperation => {
                state.serialize_field("type", "PowerUserOperation")?;
                state.serialize_field("message", "Power user operation not allowed")?;
            }
            NostrError::NostrError(e) => {
                state.serialize_field("type", "NostrError")?;
                state.serialize_field("message", &e.to_string())?;
            }
            NostrError::JsonError(e) => {
                state.serialize_field("type", "JsonError")?;
                state.serialize_field("message", &e.to_string())?;
            }
            NostrError::Nip98Error(msg) => {
                state.serialize_field("type", "Nip98Error")?;
                state.serialize_field("message", msg)?;
            }
        }
        state.end()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthEvent {
    pub id: String,
    pub pubkey: String,
    pub content: String,
    pub sig: String,
    pub created_at: i64,
    pub kind: i32,
    pub tags: Vec<Vec<String>>,
}

/// Redis key prefixes for session storage
#[cfg(feature = "redis")]
mod redis_keys {
    /// Session token -> pubkey mapping (for token lookups)
    pub const SESSION_TOKEN: &str = "nostr:session:token:";
    /// Pubkey -> user data mapping (for user storage)
    pub const USER_DATA: &str = "nostr:user:data:";
    /// Pubkey -> session token mapping (for reverse lookups)
    pub const USER_SESSION: &str = "nostr:user:session:";
}

/// In-memory replay cache for NIP-98 event IDs.
///
/// Prevents replay attacks within the 60-second token validity window by
/// tracking seen event IDs with their insertion time.  Entries older than
/// `ttl` are evicted lazily on each `check_and_insert` call.
///
/// This is intentionally a simple HashMap with TTL-based eviction rather
/// than an LRU — the 60-second window means the map stays small (bounded
/// by request rate * 120s) and a full scan every N insertions is cheap.
#[derive(Debug)]
struct Nip98ReplayCache {
    /// event_id -> wall-clock instant when first seen
    seen: HashMap<String, Instant>,
    /// How long to retain entries.  Set to 2x the token max age so that
    /// entries survive the entire window even if inserted at the boundary.
    ttl: std::time::Duration,
    /// Counter to amortise eviction — run cleanup every N inserts.
    ops_since_eviction: u32,
}

impl Nip98ReplayCache {
    fn new() -> Self {
        Self {
            seen: HashMap::new(),
            // 2x the 60-second token validity window
            ttl: std::time::Duration::from_secs(120),
            ops_since_eviction: 0,
        }
    }

    /// Returns `true` if the event ID was already seen (replay detected).
    /// If not seen, inserts it and returns `false`.
    fn check_and_insert(&mut self, event_id: &str) -> bool {
        // Amortised eviction: every 64 inserts, sweep expired entries.
        self.ops_since_eviction += 1;
        if self.ops_since_eviction >= 64 {
            self.evict_expired();
            self.ops_since_eviction = 0;
        }

        if self.seen.contains_key(event_id) {
            return true; // replay
        }

        self.seen.insert(event_id.to_string(), Instant::now());
        false
    }

    /// Remove all entries older than `self.ttl`.
    fn evict_expired(&mut self) {
        let cutoff = Instant::now() - self.ttl;
        self.seen.retain(|_, inserted_at| *inserted_at > cutoff);
    }
}

#[derive(Clone)]
pub struct NostrService {
    /// In-memory user cache (always maintained for fast access)
    users: Arc<RwLock<HashMap<String, NostrUser>>>,
    power_user_pubkeys: Vec<String>,
    token_expiry: i64,
    feature_access: Arc<RwLock<FeatureAccess>>,
    /// NIP-98 replay protection — prevents the same event ID being accepted twice
    nip98_replay_cache: Arc<std::sync::Mutex<Nip98ReplayCache>>,
    /// Redis client for persistent session storage (optional)
    #[cfg(feature = "redis")]
    redis_client: Option<RedisClient>,
}

impl NostrService {
    pub fn new() -> Self {
        let power_users = std::env::var("POWER_USER_PUBKEYS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        let token_expiry = std::env::var("AUTH_TOKEN_EXPIRY")
            .unwrap_or_else(|_| "3600".to_string())
            .parse()
            .unwrap_or(3600);

        let feature_access = Arc::new(RwLock::new(FeatureAccess::from_env()));

        // Initialize Redis client for session persistence
        #[cfg(feature = "redis")]
        let redis_client = match std::env::var("REDIS_URL") {
            Ok(url) => match RedisClient::open(url.clone()) {
                Ok(client) => {
                    info!("[NostrService] Connected to Redis for session persistence: {}",
                          url.split('@').last().unwrap_or(&url));
                    Some(client)
                }
                Err(e) => {
                    warn!("[NostrService] Failed to connect to Redis, sessions will be in-memory only: {}", e);
                    None
                }
            },
            Err(_) => {
                debug!("[NostrService] No REDIS_URL configured, sessions will be in-memory only");
                None
            }
        };

        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            power_user_pubkeys: power_users,
            feature_access,
            token_expiry,
            nip98_replay_cache: Arc::new(std::sync::Mutex::new(Nip98ReplayCache::new())),
            #[cfg(feature = "redis")]
            redis_client,
        }
    }

    /// Initialize service and restore sessions from Redis
    pub async fn initialize(&self) -> Result<usize, NostrError> {
        #[cfg(feature = "redis")]
        {
            if let Some(ref client) = self.redis_client {
                return self.restore_sessions_from_redis().await;
            }
        }
        Ok(0)
    }

    /// Restore all active sessions from Redis on startup
    #[cfg(feature = "redis")]
    async fn restore_sessions_from_redis(&self) -> Result<usize, NostrError> {
        let client = match &self.redis_client {
            Some(c) => c,
            None => return Ok(0),
        };

        let mut conn = client.get_multiplexed_async_connection().await
            .map_err(|e| NostrError::InvalidEvent(format!("Redis connection failed: {}", e)))?;

        // Scan for all user data keys
        let pattern = format!("{}*", redis_keys::USER_DATA);
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut conn)
            .await
            .unwrap_or_default();

        let mut restored_count = 0;
        let mut users = self.users.write().await;

        for key in keys {
            let user_json: Option<String> = conn.get(&key).await.unwrap_or(None);
            if let Some(json) = user_json {
                match from_json::<NostrUser>(&json) {
                    Ok(user) => {
                        // Check if session is still valid (not expired)
                        let now = time::timestamp_seconds();
                        if now - user.last_seen <= self.token_expiry {
                            info!("[NostrService] Restored session for user: {}",
                                  &user.pubkey[..16.min(user.pubkey.len())]);
                            users.insert(user.pubkey.clone(), user);
                            restored_count += 1;
                        } else {
                            debug!("[NostrService] Skipping expired session for user: {}",
                                   &key[redis_keys::USER_DATA.len()..]);
                        }
                    }
                    Err(e) => {
                        warn!("[NostrService] Failed to deserialize user from Redis: {}", e);
                    }
                }
            }
        }

        info!("[NostrService] Restored {} active sessions from Redis", restored_count);
        Ok(restored_count)
    }

    /// Persist user session to Redis
    #[cfg(feature = "redis")]
    async fn persist_session(&self, user: &NostrUser) {
        let client = match &self.redis_client {
            Some(c) => c,
            None => return,
        };

        let mut conn = match client.get_multiplexed_async_connection().await {
            Ok(c) => c,
            Err(e) => {
                warn!("[NostrService] Failed to get Redis connection for session persist: {}", e);
                return;
            }
        };

        let user_json = match to_json(user) {
            Ok(j) => j,
            Err(e) => {
                error!("[NostrService] Failed to serialize user for Redis: {}", e);
                return;
            }
        };

        let ttl_secs = self.token_expiry as u64;

        // Store user data with TTL
        let user_key = format!("{}{}", redis_keys::USER_DATA, user.pubkey);
        if let Err(e) = conn.set_ex::<_, _, ()>(&user_key, &user_json, ttl_secs).await {
            warn!("[NostrService] Failed to store user data in Redis: {}", e);
            return;
        }

        // Store session token -> pubkey mapping for token lookups
        if let Some(ref token) = user.session_token {
            let token_key = format!("{}{}", redis_keys::SESSION_TOKEN, token);
            if let Err(e) = conn.set_ex::<_, _, ()>(&token_key, &user.pubkey, ttl_secs).await {
                warn!("[NostrService] Failed to store session token in Redis: {}", e);
            }

            // Store pubkey -> session token for reverse lookups
            let session_key = format!("{}{}", redis_keys::USER_SESSION, user.pubkey);
            if let Err(e) = conn.set_ex::<_, _, ()>(&session_key, token, ttl_secs).await {
                warn!("[NostrService] Failed to store user session mapping in Redis: {}", e);
            }
        }

        debug!("[NostrService] Persisted session to Redis for user: {}",
               &user.pubkey[..16.min(user.pubkey.len())]);
    }

    /// Remove session from Redis
    #[cfg(feature = "redis")]
    async fn remove_session_from_redis(&self, pubkey: &str, token: Option<&str>) {
        let client = match &self.redis_client {
            Some(c) => c,
            None => return,
        };

        let mut conn = match client.get_multiplexed_async_connection().await {
            Ok(c) => c,
            Err(e) => {
                warn!("[NostrService] Failed to get Redis connection for session removal: {}", e);
                return;
            }
        };

        // Remove user data
        let user_key = format!("{}{}", redis_keys::USER_DATA, pubkey);
        let _: Result<(), _> = conn.del(&user_key).await;

        // Remove session token mapping
        if let Some(token) = token {
            let token_key = format!("{}{}", redis_keys::SESSION_TOKEN, token);
            let _: Result<(), _> = conn.del(&token_key).await;
        }

        // Remove user session mapping
        let session_key = format!("{}{}", redis_keys::USER_SESSION, pubkey);
        let _: Result<(), _> = conn.del(&session_key).await;

        debug!("[NostrService] Removed session from Redis for user: {}",
               &pubkey[..16.min(pubkey.len())]);
    }

    /// Check if Redis is available for session persistence
    #[cfg(feature = "redis")]
    pub fn has_redis(&self) -> bool {
        self.redis_client.is_some()
    }

    #[cfg(not(feature = "redis"))]
    pub fn has_redis(&self) -> bool {
        false
    }

    pub async fn verify_auth_event(&self, event: AuthEvent) -> Result<NostrUser, NostrError> {
        
        
        debug!(
            "Verifying auth event with id: {} and pubkey: {}",
            event.id, event.pubkey
        );

        let json_str = match to_json(&event) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to serialize auth event with id {}: {}", event.id, e);
                return Err(NostrError::JsonError(serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))));
            }
        };

        debug!(
            "Event JSON for verification (truncated): {}...",
            if json_str.len() > 100 {
                &json_str[0..100]
            } else {
                &json_str
            }
        );

        let nostr_event = match Event::from_json(&json_str) {
            Ok(e) => e,
            Err(e) => {
                error!(
                    "Failed to parse Nostr event for pubkey {}: {}",
                    event.pubkey, e
                );
                return Err(NostrError::InvalidEvent(format!(
                    "Parse error for event {}: {}",
                    event.id, e
                )));
            }
        };

        if let Err(e) = nostr_event.verify() {
            error!(
                "Signature verification failed for pubkey {}: {}",
                event.pubkey, e
            );
            return Err(NostrError::InvalidSignature);
        }

        
        let mut feature_access = self.feature_access.write().await;
        if feature_access.register_new_user(&event.pubkey) {
            info!("Registered new user with basic access: {}", event.pubkey);
        }

        let now = time::now();
        let is_power_user = self.power_user_pubkeys.contains(&event.pubkey);

        
        let session_token = Uuid::new_v4().to_string();

        let user = NostrUser {
            pubkey: event.pubkey.clone(),
            npub: nostr_event
                .pubkey
                .to_bech32()
                .map_err(|_| NostrError::NostrError(EventError::InvalidId))?,
            is_power_user,
            api_keys: ApiKeys::default(),
            last_seen: now.timestamp(),
            session_token: Some(session_token),
        };

        info!(
            "Created/updated user: pubkey={}, is_power_user={}",
            user.pubkey, user.is_power_user
        );

        // Store in memory
        let mut users = self.users.write().await;
        users.insert(user.pubkey.clone(), user.clone());
        drop(users);

        // Persist to Redis for session survival across restarts
        #[cfg(feature = "redis")]
        self.persist_session(&user).await;

        Ok(user)
    }

    pub async fn get_user(&self, pubkey: &str) -> Option<NostrUser> {
        let users = self.users.read().await;
        users.get(pubkey).cloned()
    }

    pub async fn update_user_api_keys(
        &self,
        pubkey: &str,
        api_keys: ApiKeys,
    ) -> Result<NostrUser, NostrError> {
        let mut users = self.users.write().await;

        if let Some(user) = users.get_mut(pubkey) {
            if user.is_power_user {
                return Err(NostrError::PowerUserOperation);
            }
            user.api_keys = api_keys;
            user.last_seen = time::timestamp_seconds();
            let updated_user = user.clone();
            drop(users);

            // Persist updated user to Redis
            #[cfg(feature = "redis")]
            self.persist_session(&updated_user).await;

            Ok(updated_user)
        } else {
            Err(NostrError::UserNotFound)
        }
    }

    pub async fn validate_session(&self, pubkey: &str, token: &str) -> bool {
        if let Some(user) = self.get_user(pubkey).await {
            if let Some(session_token) = user.session_token {
                let now = time::timestamp_seconds();
                if now - user.last_seen <= self.token_expiry {
                    return session_token == token;
                }
            }
        }
        false
    }

    pub async fn refresh_session(&self, pubkey: &str) -> Result<String, NostrError> {
        let mut users = self.users.write().await;

        if let Some(user) = users.get_mut(pubkey) {
            // Remove old token from Redis before creating new one
            #[cfg(feature = "redis")]
            let old_token = user.session_token.clone();

            let now = time::timestamp_seconds();
            let new_token = Uuid::new_v4().to_string();
            user.session_token = Some(new_token.clone());
            user.last_seen = now;
            let _updated_user = user.clone();
            drop(users);

            // Persist refreshed session to Redis
            #[cfg(feature = "redis")]
            {
                // Remove old token mapping
                if let Some(ref old) = old_token {
                    self.remove_session_from_redis(pubkey, Some(old)).await;
                }
                self.persist_session(&updated_user).await;
            }

            Ok(new_token)
        } else {
            Err(NostrError::UserNotFound)
        }
    }

    pub async fn logout(&self, pubkey: &str) -> Result<(), NostrError> {
        let mut users = self.users.write().await;

        if let Some(user) = users.get_mut(pubkey) {
            let _old_token = user.session_token.clone();
            user.session_token = None;
            user.last_seen = time::timestamp_seconds();
            drop(users);

            // Remove session from Redis
            #[cfg(feature = "redis")]
            self.remove_session_from_redis(pubkey, old_token.as_deref()).await;

            Ok(())
        } else {
            Err(NostrError::UserNotFound)
        }
    }

    pub async fn cleanup_sessions(&self, max_age_hours: i64) {
        let now = time::now();
        let mut users = self.users.write().await;

        users.retain(|_, user| {
            let age = now.timestamp() - user.last_seen;
            age < (max_age_hours * 3600)
        });
    }

    pub async fn is_power_user(&self, pubkey: &str) -> bool {
        if let Some(user) = self.get_user(pubkey).await {
            user.is_power_user
        } else {
            false
        }
    }

    /// Get session by token (looks up user by token)
    pub async fn get_session(&self, token: &str) -> Option<NostrUser> {
        let users = self.users.read().await;
        let token_string = token.to_string();
        users.values()
            .find(|user| {
                user.session_token.as_ref() == Some(&token_string)
            })
            .cloned()
    }

    /// Validate NIP-98 HTTP authentication from Authorization header
    /// This validates tokens for Solid server requests per NIP-98 spec.
    /// Tokens must be signed, unexpired (60s max), and match the request URL/method.
    /// # Arguments
    /// * `auth_header` - Full Authorization header (e.g., "Nostr <base64>")
    /// * `request_url` - The URL the request was made to
    /// * `request_method` - The HTTP method (GET, POST, PUT, etc.)
    /// * `request_body` - Optional request body for payload verification
    /// # Returns
    /// The authenticated NostrUser or an error
    pub async fn verify_nip98_auth(
        &self,
        auth_header: &str,
        request_url: &str,
        request_method: &str,
        request_body: Option<&str>,
    ) -> Result<NostrUser, NostrError> {
        // Parse the Authorization header
        let token = parse_auth_header(auth_header).ok_or_else(|| {
            NostrError::Nip98Error("Invalid Authorization header format".to_string())
        })?;

        // Validate the NIP-98 token
        let validation = validate_nip98_token(token, request_url, request_method, request_body)
            .map_err(|e| NostrError::Nip98Error(e.to_string()))?;

        // Replay protection: reject if this event ID was already accepted
        {
            let mut cache = self.nip98_replay_cache.lock().unwrap_or_else(|e| e.into_inner());
            if cache.check_and_insert(&validation.event_id) {
                warn!(
                    "NIP-98 replay detected: event_id={}, pubkey={}...",
                    validation.event_id,
                    &validation.pubkey[..16.min(validation.pubkey.len())]
                );
                return Err(NostrError::Nip98Error(
                    Nip98ValidationError::TokenReplayed(validation.event_id).to_string(),
                ));
            }
        }

        debug!(
            "NIP-98 token validated for pubkey: {}..., url: {}, method: {}",
            &validation.pubkey[..16.min(validation.pubkey.len())],
            validation.url,
            validation.method
        );

        // Get or create the user
        let user = self.get_or_create_user_from_pubkey(&validation.pubkey).await?;

        info!(
            "NIP-98 auth successful for pubkey: {}, is_power_user: {}",
            user.pubkey, user.is_power_user
        );

        Ok(user)
    }

    /// Get existing user or create a new one from pubkey
    async fn get_or_create_user_from_pubkey(&self, pubkey: &str) -> Result<NostrUser, NostrError> {
        // Check if user exists
        if let Some(_user) = self.get_user(pubkey).await {
            // Update last_seen
            let mut users = self.users.write().await;
            if let Some(user) = users.get_mut(pubkey) {
                user.last_seen = time::timestamp_seconds();
                let updated_user = user.clone();
                drop(users);

                // Persist updated last_seen to Redis
                #[cfg(feature = "redis")]
                self.persist_session(&updated_user).await;

                return Ok(updated_user);
            }
            return Err(NostrError::UserNotFound);
        }

        // Register new user with feature access
        let mut feature_access = self.feature_access.write().await;
        if feature_access.register_new_user(pubkey) {
            info!("Registered new user via NIP-98 with basic access: {}", pubkey);
        }
        drop(feature_access);

        let is_power_user = self.power_user_pubkeys.contains(&pubkey.to_string());
        let session_token = Uuid::new_v4().to_string();

        // Convert hex pubkey to npub (bech32)
        let npub = match PublicKey::from_hex(pubkey) {
            Ok(pk) => pk.to_bech32().unwrap_or_else(|_| pubkey.to_string()),
            Err(_) => {
                warn!("Could not convert pubkey to npub: {}", pubkey);
                pubkey.to_string()
            }
        };

        let user = NostrUser {
            pubkey: pubkey.to_string(),
            npub,
            is_power_user,
            api_keys: ApiKeys::default(),
            last_seen: time::timestamp_seconds(),
            session_token: Some(session_token),
        };

        // Store the new user
        let mut users = self.users.write().await;
        users.insert(pubkey.to_string(), user.clone());
        drop(users);

        // Persist new user session to Redis
        #[cfg(feature = "redis")]
        self.persist_session(&user).await;

        info!(
            "Created new user via NIP-98: pubkey={}, is_power_user={}",
            user.pubkey, user.is_power_user
        );

        Ok(user)
    }

    /// Validate NIP-98 token and return just the validation result (no user creation)
    /// Use this for stateless validation when you only need to verify the token.
    pub fn validate_nip98_token_only(
        &self,
        auth_header: &str,
        request_url: &str,
        request_method: &str,
        request_body: Option<&str>,
    ) -> Result<Nip98ValidationResult, NostrError> {
        let token = parse_auth_header(auth_header).ok_or_else(|| {
            NostrError::Nip98Error("Invalid Authorization header format".to_string())
        })?;

        validate_nip98_token(token, request_url, request_method, request_body)
            .map_err(|e| NostrError::Nip98Error(e.to_string()))
    }
}

impl Default for NostrService {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Nip98ReplayCache unit tests ----

    #[test]
    fn test_replay_cache_insert_returns_false_on_first_see() {
        let mut cache = Nip98ReplayCache::new();
        assert!(!cache.check_and_insert("event-001"));
    }

    #[test]
    fn test_replay_cache_insert_returns_true_on_replay() {
        let mut cache = Nip98ReplayCache::new();
        assert!(!cache.check_and_insert("event-002"));
        assert!(cache.check_and_insert("event-002")); // replay
    }

    #[test]
    fn test_replay_cache_distinct_events_not_confused() {
        let mut cache = Nip98ReplayCache::new();
        assert!(!cache.check_and_insert("event-a"));
        assert!(!cache.check_and_insert("event-b"));
        assert!(!cache.check_and_insert("event-c"));
        assert!(cache.check_and_insert("event-a")); // replay of a
        assert!(!cache.check_and_insert("event-d")); // new
    }

    #[test]
    fn test_replay_cache_eviction_clears_old_entries() {
        let mut cache = Nip98ReplayCache {
            seen: HashMap::new(),
            ttl: std::time::Duration::from_millis(0), // immediate expiry
            ops_since_eviction: 0,
        };
        cache.check_and_insert("old-event");
        // Force eviction
        cache.evict_expired();
        // After eviction, the event should be gone (ttl=0 means everything is old)
        assert!(!cache.check_and_insert("old-event")); // no longer a replay
    }

    #[test]
    fn test_replay_cache_default_ttl_is_120s() {
        let cache = Nip98ReplayCache::new();
        assert_eq!(cache.ttl, std::time::Duration::from_secs(120));
    }

    #[test]
    fn test_replay_cache_amortised_eviction_interval() {
        let mut cache = Nip98ReplayCache::new();
        // Insert 63 events — no eviction yet
        for i in 0..63 {
            cache.check_and_insert(&format!("e{}", i));
        }
        assert_eq!(cache.seen.len(), 63);
        // 64th insert triggers eviction (but entries are still fresh so nothing removed)
        cache.check_and_insert("e63");
        assert_eq!(cache.ops_since_eviction, 0); // reset after eviction
    }

    // ---- NostrService construction ----

    #[tokio::test]
    async fn test_nostr_service_default_construction() {
        let service = NostrService::new();
        // No users initially
        let user = service.get_user("nonexistent").await;
        assert!(user.is_none());
        assert!(!service.has_redis());
    }

    // ---- Session validation ----

    #[tokio::test]
    async fn test_validate_session_nonexistent_user() {
        let service = NostrService::new();
        let valid = service.validate_session("no-such-pubkey", "no-such-token").await;
        assert!(!valid);
    }

    // ---- Power user detection ----

    #[tokio::test]
    async fn test_is_power_user_returns_false_for_unknown() {
        let service = NostrService::new();
        assert!(!service.is_power_user("unknown-pubkey").await);
    }

    // ---- Session lookup by token ----

    #[tokio::test]
    async fn test_get_session_nonexistent_token() {
        let service = NostrService::new();
        let session = service.get_session("nonexistent-token").await;
        assert!(session.is_none());
    }

    // ---- Update API keys for nonexistent user ----

    #[tokio::test]
    async fn test_update_api_keys_user_not_found() {
        let service = NostrService::new();
        let result = service
            .update_user_api_keys("no-such-user", ApiKeys::default())
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            NostrError::UserNotFound => {}
            other => panic!("Expected UserNotFound, got {:?}", other),
        }
    }

    // ---- Refresh session for nonexistent user ----

    #[tokio::test]
    async fn test_refresh_session_user_not_found() {
        let service = NostrService::new();
        let result = service.refresh_session("no-such-user").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            NostrError::UserNotFound => {}
            other => panic!("Expected UserNotFound, got {:?}", other),
        }
    }

    // ---- Logout for nonexistent user ----

    #[tokio::test]
    async fn test_logout_user_not_found() {
        let service = NostrService::new();
        let result = service.logout("no-such-user").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            NostrError::UserNotFound => {}
            other => panic!("Expected UserNotFound, got {:?}", other),
        }
    }

    // ---- Cleanup sessions ----

    #[tokio::test]
    async fn test_cleanup_sessions_removes_old() {
        let service = NostrService::new();
        // Manually insert a user with very old last_seen
        {
            let mut users = service.users.write().await;
            users.insert(
                "old-user".to_string(),
                NostrUser {
                    pubkey: "old-user".to_string(),
                    npub: "npub1old".to_string(),
                    is_power_user: false,
                    api_keys: ApiKeys::default(),
                    last_seen: 0, // epoch = very old
                    session_token: Some("token".to_string()),
                },
            );
        }
        // Should exist before cleanup
        assert!(service.get_user("old-user").await.is_some());
        // Cleanup with 1 hour max age
        service.cleanup_sessions(1).await;
        // Should be removed (last_seen=0 is far more than 1 hour ago)
        assert!(service.get_user("old-user").await.is_none());
    }

    // ---- NIP-98 token validation (stateless, invalid header) ----

    #[test]
    fn test_validate_nip98_token_only_invalid_header() {
        let service = NostrService::new();
        let result = service.validate_nip98_token_only(
            "Bearer invalid-not-nostr",
            "https://example.com/api",
            "GET",
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_nip98_token_only_empty_header() {
        let service = NostrService::new();
        let result = service.validate_nip98_token_only(
            "",
            "https://example.com/api",
            "GET",
            None,
        );
        assert!(result.is_err());
    }

    // ---- NostrError serialization ----

    #[test]
    fn test_nostr_error_serialization() {
        let errors: Vec<NostrError> = vec![
            NostrError::InvalidEvent("bad event".to_string()),
            NostrError::InvalidSignature,
            NostrError::UserNotFound,
            NostrError::InvalidToken,
            NostrError::SessionExpired,
            NostrError::PowerUserOperation,
            NostrError::Nip98Error("replay".to_string()),
        ];

        for err in errors {
            let json = serde_json::to_string(&err).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert!(parsed.get("type").is_some(), "Missing 'type' field in {:?}", err);
            assert!(parsed.get("message").is_some(), "Missing 'message' field in {:?}", err);
        }
    }

    // ---- AuthEvent deserialization ----

    #[test]
    fn test_auth_event_deserialization() {
        let json = r#"{
            "id": "event123",
            "pubkey": "aabbccdd",
            "content": "auth",
            "sig": "sig123",
            "created_at": 1700000000,
            "kind": 27235,
            "tags": [["u", "https://example.com"], ["method", "GET"]]
        }"#;
        let event: AuthEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.id, "event123");
        assert_eq!(event.kind, 27235);
        assert_eq!(event.tags.len(), 2);
        assert_eq!(event.tags[0][0], "u");
    }

    // ---- Initialize without Redis ----

    #[tokio::test]
    async fn test_initialize_without_redis() {
        let service = NostrService::new();
        let result = service.initialize().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }
}
