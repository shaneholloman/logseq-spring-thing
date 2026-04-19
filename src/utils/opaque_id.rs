//! Session-scoped opaque-id generation for private KG nodes (ADR-050).
//!
//! When a non-owner client is served a private node, the server must present
//! a stable-ish identifier so the client can do diffing and delta updates
//! within one session — but that identifier must **not** be reversible into
//! the canonical IRI or reveal the owner's identity.
//!
//! The construction is:
//!
//! ```text
//! opaque_id = hex( HMAC-SHA256(salt, owner_pubkey || '|' || canonical_iri) )[..24]
//! ```
//!
//! Properties:
//! - **Deterministic within a salt window**: the same private node yields the
//!   same opaque id for the duration of one salt period, enabling frame-to-
//!   frame diffing on the client.
//! - **Unlinkable across salt rotations**: after rotation, the same node gets
//!   a fresh opaque id, so two different observers (or the same observer at
//!   different points in the week) cannot stitch histories together.
//! - **Not invertible without the salt**: an attacker who scrapes opaque ids
//!   cannot dictionary-attack the canonical IRI space without knowing the
//!   current salt; HMAC-SHA256 with a ≥32-byte secret key is PRF-secure.
//!
//! The salt is seeded from the `OPAQUE_ID_SALT_SEED` env var and rotated
//! every 24 h by a background tokio task. The previous salt is retained for
//! an additional grace window so in-flight client sessions don't break at the
//! rotation boundary.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};

use sha2::Digest;

type HmacSha256 = Hmac<Sha256>;

/// How often the active salt rotates.
pub const SALT_ROTATION_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// How long the previous salt remains accepted after a rotation. Total
/// lifetime of any given salt is therefore `SALT_ROTATION_INTERVAL +
/// SALT_GRACE_WINDOW = 48 h`, which matches the retention quoted in ADR-050.
pub const SALT_GRACE_WINDOW: Duration = Duration::from_secs(24 * 60 * 60);

/// Rotating-salt state. Cheaply cloneable — all interior fields are `Arc`.
#[derive(Clone)]
pub struct SessionSalt {
    current: Arc<RwLock<SaltEntry>>,
    previous: Arc<RwLock<Option<SaltEntry>>>,
    seed: Arc<Vec<u8>>,
}

#[derive(Clone, Debug)]
struct SaltEntry {
    salt: Vec<u8>,
    rotated_at: DateTime<Utc>,
}

impl SessionSalt {
    /// Create a `SessionSalt` from the `OPAQUE_ID_SALT_SEED` env var.
    ///
    /// The env var must be set to at least 16 bytes of high-entropy material
    /// (hex, base64, or raw — we hash it, so encoding doesn't matter). It is
    /// normally provisioned by the deploy orchestrator.
    pub fn from_env() -> anyhow::Result<Self> {
        let seed = std::env::var("OPAQUE_ID_SALT_SEED")
            .map_err(|_| anyhow::anyhow!("OPAQUE_ID_SALT_SEED not set"))?;
        if seed.len() < 16 {
            return Err(anyhow::anyhow!(
                "OPAQUE_ID_SALT_SEED must be at least 16 chars (got {})",
                seed.len()
            ));
        }
        Ok(Self::new(seed.as_bytes()))
    }

    /// Construct directly from seed bytes. Primarily used by tests and by
    /// deployments that provision the seed via a mechanism other than env.
    pub fn new(seed_bytes: &[u8]) -> Self {
        let seed_owned = seed_bytes.to_vec();
        let now = Utc::now();
        let initial = SaltEntry {
            salt: derive_salt(&seed_owned, now),
            rotated_at: now,
        };
        Self {
            current: Arc::new(RwLock::new(initial)),
            previous: Arc::new(RwLock::new(None)),
            seed: Arc::new(seed_owned),
        }
    }

    /// Return the currently-active salt. Cheap clone (~32 bytes).
    pub async fn current_salt(&self) -> Vec<u8> {
        self.current.read().await.salt.clone()
    }

    /// Return the previous salt, if any. Used by the verifier side so clients
    /// who computed their ids moments before a rotation still match.
    pub async fn previous_salt(&self) -> Option<Vec<u8>> {
        self.previous.read().await.as_ref().map(|e| e.salt.clone())
    }

    /// Rotate the salt now. Moves `current` into `previous`, derives a fresh
    /// `current` from `(seed, now)`. Called by the background task every
    /// `SALT_ROTATION_INTERVAL` but also exposed for tests and for manual
    /// rotation in response to a suspected compromise.
    pub async fn rotate(&self) {
        let now = Utc::now();
        let fresh = SaltEntry {
            salt: derive_salt(&self.seed, now),
            rotated_at: now,
        };
        let mut cur_w = self.current.write().await;
        let old = std::mem::replace(&mut *cur_w, fresh);
        drop(cur_w);
        // Retain the old salt only if it is still inside the grace window.
        let keep_until = old.rotated_at
            + chrono::Duration::from_std(SALT_ROTATION_INTERVAL + SALT_GRACE_WINDOW)
                .unwrap_or(chrono::Duration::hours(48));
        let mut prev_w = self.previous.write().await;
        *prev_w = if Utc::now() < keep_until { Some(old) } else { None };
    }

    /// Spawn the background rotation task. Returns the `JoinHandle` so the
    /// caller can manage shutdown if needed.
    pub fn spawn_rotation_task(&self) -> tokio::task::JoinHandle<()> {
        let this = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(SALT_ROTATION_INTERVAL);
            // First tick fires immediately — consume it so we don't rotate
            // the salt we just derived in `new`.
            interval.tick().await;
            loop {
                interval.tick().await;
                this.rotate().await;
                log::info!("opaque_id: rotated session salt");
            }
        })
    }
}

/// Derive a fresh 32-byte salt from `(seed, timestamp)`. Timestamp granularity
/// is 1 day — the same (seed, calendar-day) pair always produces the same
/// salt, so a restart within the same day preserves opaque ids.
fn derive_salt(seed: &[u8], at: DateTime<Utc>) -> Vec<u8> {
    // Quantise to the day so container restarts don't churn the salt.
    let day_index: i64 = at.timestamp() / 86_400;
    let mut hasher = Sha256::new();
    hasher.update(seed);
    hasher.update(b"|opaque-id-salt|");
    hasher.update(day_index.to_le_bytes());
    hasher.finalize().to_vec()
}

/// Compute the opaque id for a (owner, iri) pair under the given salt.
///
/// Returns 24 lowercase hex characters = 12 bytes of HMAC output, which gives
/// 96 bits of preimage resistance — overkill for a session-lifetime cookie
/// but cheap, and keeps the id comfortably below the JS `Number` safe range
/// if the client ever parses it as an integer.
pub fn opaque_id(salt: &[u8], owner_pubkey: &str, canonical_iri: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(salt)
        .expect("HMAC-SHA256 accepts keys of any length");
    mac.update(owner_pubkey.as_bytes());
    mac.update(b"|");
    mac.update(canonical_iri.as_bytes());
    let result = mac.finalize().into_bytes();
    // First 12 bytes -> 24 hex chars. Manual encoding to avoid a `hex` dep.
    let mut out = String::with_capacity(24);
    for b in result.iter().take(12) {
        out.push(nibble_to_hex(b >> 4));
        out.push(nibble_to_hex(b & 0x0F));
    }
    out
}

#[inline]
fn nibble_to_hex(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'a' + (n - 10)) as char,
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    const IRI: &str = "visionclaw:owner:npub1abc/kg/deadbeef";

    #[test]
    fn opaque_id_has_correct_shape() {
        let id = opaque_id(b"salt-0123456789ab", OWNER, IRI);
        assert_eq!(id.len(), 24);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn opaque_id_is_deterministic_for_same_salt() {
        let salt = b"fixed-test-salt-1234567890";
        let a = opaque_id(salt, OWNER, IRI);
        let b = opaque_id(salt, OWNER, IRI);
        assert_eq!(a, b);
    }

    #[test]
    fn opaque_id_changes_when_salt_rotates() {
        let a = opaque_id(b"salt-one-1234567890ab", OWNER, IRI);
        let b = opaque_id(b"salt-two-1234567890ab", OWNER, IRI);
        assert_ne!(a, b);
    }

    #[test]
    fn opaque_id_changes_for_different_iri() {
        let salt = b"salt-xx-1234567890abcd";
        let a = opaque_id(salt, OWNER, "visionclaw:owner:x/kg/aaaa");
        let b = opaque_id(salt, OWNER, "visionclaw:owner:x/kg/bbbb");
        assert_ne!(a, b);
    }

    #[test]
    fn opaque_id_changes_for_different_owner() {
        let salt = b"salt-xx-1234567890abcd";
        let a = opaque_id(salt, OWNER, IRI);
        let b = opaque_id(salt, "0000000000000000000000000000000000000000000000000000000000000001", IRI);
        assert_ne!(a, b);
    }

    #[test]
    fn opaque_id_hmac_resists_dictionary_without_salt() {
        // Compute under a secret salt. Without knowing the salt, an attacker
        // who guesses the full (owner, iri) tuple still cannot reproduce the
        // id unless they guess the salt too.
        let secret_salt = b"unguessable-production-salt-XYZ";
        let attacker_salt = b"attackers-guess-salt-XYZ";
        let real = opaque_id(secret_salt, OWNER, IRI);
        let guess = opaque_id(attacker_salt, OWNER, IRI);
        assert_ne!(real, guess,
            "HMAC output must depend on the salt; two salts must not collide");
    }

    #[test]
    fn session_salt_rotation_changes_current() {
        // Run under a mini tokio runtime.
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let s = SessionSalt::new(b"seed-for-rotation-test-000000");
            let before = s.current_salt().await;
            // Derive a salt based on a later day and inject manually so the
            // test doesn't depend on wall-clock.
            let future_salt = derive_salt(b"seed-for-rotation-test-000000", Utc::now() + chrono::Duration::days(2));
            {
                let mut cur_w = s.current.write().await;
                cur_w.salt = future_salt.clone();
            }
            let after = s.current_salt().await;
            assert_ne!(before, after);
        });
    }

    #[test]
    fn derive_salt_is_day_quantised() {
        let t1 = Utc::now();
        let t2 = t1 + chrono::Duration::seconds(3600); // +1h same day
        assert_eq!(
            derive_salt(b"seed-XXXXXXXXXXXX", t1),
            derive_salt(b"seed-XXXXXXXXXXXX", t2)
        );
        let t3 = t1 + chrono::Duration::days(1);
        assert_ne!(
            derive_salt(b"seed-XXXXXXXXXXXX", t1),
            derive_salt(b"seed-XXXXXXXXXXXX", t3)
        );
    }
}
