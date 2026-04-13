//! Nostr Bridge: JSS relay → DreamLab Forum relay.
//!
//! Subscribes to kind 30001 bead provenance events on the JSS relay and
//! republishes them as NIP-29 group messages (kind 9) to the forum relay.
//! Re-signs with its own keypair for the forum whitelist; preserves the
//! original event ID in a `source_event` tag.
//!
//! Reconnects with exponential backoff (5 s → 300 s); resets after 60 s of
//! healthy streaming. Exposes a `BridgeHealth` handle for external monitoring.
//!
//! ```ignore
//! let bridge = NostrBridge::from_env().unwrap();
//! let health = bridge.health();
//! tokio::spawn(bridge.run());
//! assert!(!health.is_connected()); // not yet
//! ```

use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
use nostr_sdk::prelude::*;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const INITIAL_BACKOFF_SECS: u64 = 5;
const MAX_BACKOFF_SECS: u64 = 300;
const BACKOFF_MULTIPLIER: f64 = 2.0;
/// If a connection lasted longer than this, reset backoff on reconnect.
const HEALTHY_CONNECTION_SECS: u64 = 60;

/// Cheaply cloneable handle for querying bridge health from external code.
#[derive(Clone)]
pub struct BridgeHealth {
    connected: Arc<AtomicBool>,
    last_event_at: Arc<Mutex<Option<Instant>>>,
}

impl BridgeHealth {
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn last_event_age_secs(&self) -> Option<u64> {
        self.last_event_at
            .lock()
            .ok()
            .and_then(|guard| guard.map(|t| t.elapsed().as_secs()))
    }
}

pub struct NostrBridge {
    keys: Keys,
    jss_relay_url: String,
    forum_relay_url: String,
    connected: Arc<AtomicBool>,
    last_event_at: Arc<Mutex<Option<Instant>>>,
}

impl NostrBridge {
    /// Load from environment. Returns `None` if required vars are absent.
    pub fn from_env() -> Option<Self> {
        let privkey = std::env::var("VISIONCLAW_NOSTR_PRIVKEY")
            .ok()
            .filter(|s| !s.is_empty())?;
        let forum_relay_url = std::env::var("FORUM_RELAY_URL")
            .ok()
            .filter(|s| !s.is_empty())?;
        let jss_relay_url = std::env::var("JSS_RELAY_URL")
            .unwrap_or_else(|_| "ws://jss:3030/relay".to_string());

        // Validate relay URL schemes to prevent SSRF via env var injection.
        if !jss_relay_url.starts_with("ws://") && !jss_relay_url.starts_with("wss://") {
            error!("[NostrBridge] JSS_RELAY_URL must start with ws:// or wss://: {jss_relay_url}");
            return None;
        }
        if !forum_relay_url.starts_with("ws://") && !forum_relay_url.starts_with("wss://") {
            error!("[NostrBridge] FORUM_RELAY_URL must start with ws:// or wss://: {forum_relay_url}");
            return None;
        }

        let secret_key = SecretKey::from_hex(&privkey)
            .map_err(|e| error!("[NostrBridge] Invalid VISIONCLAW_NOSTR_PRIVKEY: {e}"))
            .ok()?;

        Some(Self {
            keys: Keys::new(secret_key),
            jss_relay_url,
            forum_relay_url,
            connected: Arc::new(AtomicBool::new(false)),
            last_event_at: Arc::new(Mutex::new(None)),
        })
    }

    /// Return a cloneable health handle. Call *before* `run()` consumes self.
    pub fn health(&self) -> BridgeHealth {
        BridgeHealth {
            connected: Arc::clone(&self.connected),
            last_event_at: Arc::clone(&self.last_event_at),
        }
    }

    /// Run the bridge loop indefinitely with exponential backoff reconnection.
    /// Call via `tokio::spawn(bridge.run())`.
    pub async fn run(self) {
        info!(
            target: "bead_bridge",
            "[NostrBridge] Starting {} → {}", self.jss_relay_url, self.forum_relay_url
        );
        let mut backoff_secs = INITIAL_BACKOFF_SECS;
        loop {
            let started = Instant::now();
            match self.run_once().await {
                Ok(()) => {
                    warn!(
                        target: "bead_bridge",
                        "[NostrBridge] Stream ended unexpectedly, reconnecting in {backoff_secs}s"
                    );
                }
                Err(e) => {
                    warn!(
                        target: "bead_bridge",
                        "[NostrBridge] Connection lost ({e}), reconnecting in {backoff_secs}s"
                    );
                }
            }
            self.connected.store(false, Ordering::Relaxed);
            // Reset backoff if the connection was healthy for a sustained period.
            if started.elapsed().as_secs() > HEALTHY_CONNECTION_SECS {
                backoff_secs = INITIAL_BACKOFF_SECS;
            }
            tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
            backoff_secs =
                ((backoff_secs as f64 * BACKOFF_MULTIPLIER) as u64).min(MAX_BACKOFF_SECS);
        }
    }

    async fn run_once(&self) -> Result<(), String> {
        let (jss_stream, _) = connect_async(&self.jss_relay_url)
            .await
            .map_err(|e| format!("JSS connect failed: {e}"))?;

        let (mut jss_write, mut jss_read) = jss_stream.split();

        // Subscribe to kind 30001 bead provenance events only.
        jss_write
            .send(Message::Text(
                json!(["REQ", "bridge-sub", {"kinds": [30001]}]).to_string(),
            ))
            .await
            .map_err(|e| format!("REQ send failed: {e}"))?;

        self.connected.store(true, Ordering::Relaxed);
        info!(
            target: "bead_bridge",
            "[NostrBridge] Subscribed to JSS relay for kind 30001"
        );

        while let Some(msg) = jss_read.next().await {
            match msg {
                Ok(Message::Text(txt)) => {
                    if let Ok(parsed) = serde_json::from_str::<Value>(&txt) {
                        // ["EVENT", "<sub_id>", <event_object>]
                        if parsed[0] == "EVENT" {
                            if let Some(event_obj) = parsed.get(2) {
                                self.forward_to_forum(event_obj).await;
                            }
                        }
                    }
                }
                Ok(Message::Close(_)) => return Err("JSS relay closed connection".to_string()),
                Err(e) => return Err(format!("WebSocket error: {e}")),
                _ => {}
            }
        }

        Err("JSS relay stream ended".to_string())
    }

    async fn forward_to_forum(&self, original: &Value) {
        // Verify the Nostr event signature before forwarding — prevents injection
        // by any party that can write to the JSS relay.
        let event_json = match serde_json::to_string(original) {
            Ok(s) => s,
            Err(e) => { warn!(target: "bead_bridge", "[NostrBridge] Serialise failed: {e}"); return; }
        };
        let verified = match Event::from_json(&event_json) {
            Err(e) => { warn!(target: "bead_bridge", "[NostrBridge] Unparseable event: {e}"); return; }
            Ok(ev) => ev,
        };
        if let Err(e) = verified.verify() {
            warn!(target: "bead_bridge", "[NostrBridge] Bad signature: {e}");
            return;
        }

        let bead_id = tag_value(original, "bead_id").unwrap_or("unknown");
        let brief_id = tag_value(original, "brief_id").unwrap_or("-");
        let debrief_path = tag_value(original, "debrief_path").unwrap_or("-");
        let source_event_id = original["id"].as_str().unwrap_or("");

        let content = format!("bead:{bead_id} brief:{brief_id} path:{debrief_path}");

        let tags = vec![
            Tag::custom(
                TagKind::Custom("h".into()),
                vec!["visionclaw-activity".to_string()],
            ),
            Tag::custom(
                TagKind::Custom("bead_id".into()),
                vec![bead_id.to_string()],
            ),
            Tag::custom(
                TagKind::Custom("source_event".into()),
                vec![source_event_id.to_string()],
            ),
        ];

        let event = match EventBuilder::new(Kind::Custom(9), &content)
            .tags(tags)
            .sign_with_keys(&self.keys)
        {
            Ok(e) => e,
            Err(e) => {
                error!(
                    target: "bead_bridge",
                    "[NostrBridge] Failed to sign forum event: {e}"
                );
                return;
            }
        };

        match self.send_to_forum(&event).await {
            Ok(()) => {
                if let Ok(mut guard) = self.last_event_at.lock() {
                    *guard = Some(Instant::now());
                }
                info!(
                    target: "bead_bridge",
                    "[NostrBridge] Forwarded bead_id={bead_id} source_event={source_event_id}"
                );
            }
            Err(e) => warn!(
                target: "bead_bridge",
                "[NostrBridge] Failed bead_id={bead_id} source_event={source_event_id}: {e}"
            ),
        }
    }

    async fn send_to_forum(&self, event: &Event) -> Result<(), String> {
        let (ws_stream, _) = connect_async(&self.forum_relay_url)
            .await
            .map_err(|e| format!("forum connect: {e}"))?;

        let (mut write, mut read) = ws_stream.split();

        write
            .send(Message::Text(json!(["EVENT", event]).to_string()))
            .await
            .map_err(|e| format!("send failed: {e}"))?;

        tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(Ok(Message::Text(txt))) = read.next().await {
                if let Ok(parsed) = serde_json::from_str::<Value>(&txt) {
                    if parsed[0] == "OK" {
                        return if parsed[2].as_bool().unwrap_or(false) {
                            Ok(())
                        } else {
                            Err(format!("forum rejected: {}", parsed[3]))
                        };
                    }
                }
            }
            Err("forum relay closed without OK".to_string())
        })
        .await
        .map_err(|_| "forum relay timeout".to_string())?
    }
}

/// Extract the first value of a named tag from a raw Nostr event JSON object.
fn tag_value<'a>(event: &'a Value, tag_name: &str) -> Option<&'a str> {
    event["tags"].as_array()?.iter().find_map(|t| {
        if t[0].as_str() == Some(tag_name) {
            t[1].as_str()
        } else {
            None
        }
    })
}

/// Compute backoff duration for a given iteration (0-indexed).
/// Public for testing only via `#[cfg(test)]`.
fn compute_backoff(iteration: u32) -> u64 {
    let delay = (INITIAL_BACKOFF_SECS as f64) * BACKOFF_MULTIPLIER.powi(iteration as i32);
    (delay as u64).min(MAX_BACKOFF_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── from_env ───────────────────────────────────────────────────────

    #[test]
    fn from_env_returns_none_without_required_vars() {
        // GIVEN: no VISIONCLAW_NOSTR_PRIVKEY
        std::env::remove_var("VISIONCLAW_NOSTR_PRIVKEY");
        std::env::remove_var("FORUM_RELAY_URL");

        // WHEN/THEN: None
        assert!(NostrBridge::from_env().is_none());
    }

    #[test]
    fn from_env_returns_none_without_forum_relay() {
        // GIVEN: privkey set but no FORUM_RELAY_URL
        std::env::set_var(
            "VISIONCLAW_NOSTR_PRIVKEY",
            "0000000000000000000000000000000000000000000000000000000000000001",
        );
        std::env::remove_var("FORUM_RELAY_URL");

        let result = NostrBridge::from_env();
        assert!(result.is_none());

        std::env::remove_var("VISIONCLAW_NOSTR_PRIVKEY");
    }

    #[test]
    fn from_env_rejects_non_ws_jss_url() {
        // GIVEN: valid privkey, valid forum URL, but HTTP jss URL
        std::env::set_var(
            "VISIONCLAW_NOSTR_PRIVKEY",
            "0000000000000000000000000000000000000000000000000000000000000001",
        );
        std::env::set_var("FORUM_RELAY_URL", "wss://relay.dreamlab-ai.com");
        std::env::set_var("JSS_RELAY_URL", "http://evil.com/jss");

        let result = NostrBridge::from_env();
        assert!(result.is_none());

        std::env::remove_var("VISIONCLAW_NOSTR_PRIVKEY");
        std::env::remove_var("FORUM_RELAY_URL");
        std::env::remove_var("JSS_RELAY_URL");
    }

    #[test]
    fn from_env_rejects_non_ws_forum_url() {
        // GIVEN: valid privkey and jss but HTTP forum URL
        std::env::set_var(
            "VISIONCLAW_NOSTR_PRIVKEY",
            "0000000000000000000000000000000000000000000000000000000000000001",
        );
        std::env::set_var("FORUM_RELAY_URL", "http://evil.com/forum");
        std::env::set_var("JSS_RELAY_URL", "ws://jss:3030/relay");

        let result = NostrBridge::from_env();
        assert!(result.is_none());

        std::env::remove_var("VISIONCLAW_NOSTR_PRIVKEY");
        std::env::remove_var("FORUM_RELAY_URL");
        std::env::remove_var("JSS_RELAY_URL");
    }

    // ── tag_value ──────────────────────────────────────────────────────

    #[test]
    fn tag_value_extracts_correct_tag() {
        // GIVEN: a Nostr-style event JSON with tags
        let event: Value = serde_json::json!({
            "tags": [
                ["bead_id", "bead-42"],
                ["brief_id", "brief-7"],
                ["debrief_path", "/out/debrief"]
            ]
        });

        // WHEN/THEN: tag_value extracts the correct value
        assert_eq!(tag_value(&event, "bead_id"), Some("bead-42"));
        assert_eq!(tag_value(&event, "brief_id"), Some("brief-7"));
        assert_eq!(tag_value(&event, "debrief_path"), Some("/out/debrief"));
    }

    #[test]
    fn tag_value_returns_none_for_missing_tag() {
        let event: Value = serde_json::json!({
            "tags": [["bead_id", "bead-1"]]
        });

        assert_eq!(tag_value(&event, "nonexistent"), None);
    }

    #[test]
    fn tag_value_returns_none_for_no_tags() {
        let event: Value = serde_json::json!({});
        assert_eq!(tag_value(&event, "bead_id"), None);
    }

    #[test]
    fn tag_value_returns_none_for_empty_tags_array() {
        let event: Value = serde_json::json!({"tags": []});
        assert_eq!(tag_value(&event, "bead_id"), None);
    }

    // ── BridgeHealth ───────────────────────────────────────────────────

    #[test]
    fn bridge_health_is_connected_initially_false() {
        // GIVEN: fresh bridge health state
        let health = BridgeHealth {
            connected: Arc::new(AtomicBool::new(false)),
            last_event_at: Arc::new(Mutex::new(None)),
        };

        // THEN: not connected
        assert!(!health.is_connected());
    }

    #[test]
    fn bridge_health_reflects_connected_state() {
        // GIVEN: connected set to true
        let connected = Arc::new(AtomicBool::new(true));
        let health = BridgeHealth {
            connected: connected.clone(),
            last_event_at: Arc::new(Mutex::new(None)),
        };

        // THEN: is_connected returns true
        assert!(health.is_connected());

        // WHEN: set to false
        connected.store(false, Ordering::Relaxed);
        assert!(!health.is_connected());
    }

    #[test]
    fn bridge_health_last_event_age_none_initially() {
        let health = BridgeHealth {
            connected: Arc::new(AtomicBool::new(false)),
            last_event_at: Arc::new(Mutex::new(None)),
        };

        assert!(health.last_event_age_secs().is_none());
    }

    #[test]
    fn bridge_health_last_event_age_returns_elapsed() {
        let health = BridgeHealth {
            connected: Arc::new(AtomicBool::new(true)),
            last_event_at: Arc::new(Mutex::new(Some(Instant::now()))),
        };

        // Age should be very small (< 1 second)
        let age = health.last_event_age_secs().unwrap();
        assert!(age < 2);
    }

    // ── Backoff calculation ────────────────────────────────────────────

    #[test]
    fn backoff_doubles_correctly() {
        // GIVEN: initial backoff of 5s, multiplier 2.0
        // WHEN: computing successive backoffs
        assert_eq!(compute_backoff(0), 5);   // 5 * 2^0 = 5
        assert_eq!(compute_backoff(1), 10);  // 5 * 2^1 = 10
        assert_eq!(compute_backoff(2), 20);  // 5 * 2^2 = 20
        assert_eq!(compute_backoff(3), 40);  // 5 * 2^3 = 40
    }

    #[test]
    fn backoff_caps_at_max_backoff_secs() {
        // GIVEN: large iteration number
        // WHEN: computing backoff
        let capped = compute_backoff(10); // 5 * 2^10 = 5120 > 300

        // THEN: capped at MAX_BACKOFF_SECS
        assert_eq!(capped, MAX_BACKOFF_SECS);
    }

    #[test]
    fn backoff_constants_are_consistent() {
        assert!(INITIAL_BACKOFF_SECS < MAX_BACKOFF_SECS);
        assert!(BACKOFF_MULTIPLIER > 1.0);
        assert!(HEALTHY_CONNECTION_SECS > 0);
    }
}
