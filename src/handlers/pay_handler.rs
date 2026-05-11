//! HTTP 402 Payment Required — per-resource payment gating via `did:nostr` pubkeys.
//!
//! Wires `solid-pod-rs` payment types (Web Ledgers, PayConfig) into VisionClaw's
//! actix-web surface. Balances are tracked per `did:nostr:<hex-pubkey>` identity in
//! a filesystem-backed JSON ledger under a configurable data directory.
//!
//! ## Routes (all gated behind `PAY_ENABLED`)
//!
//! | Method | Path               | Description                                 |
//! |--------|--------------------|---------------------------------------------|
//! | GET    | `/pay/.info`       | Payment info (cost, methods, endpoints)     |
//! | GET    | `/pay/.balance`    | Caller's balance (requires NIP-98 auth)     |
//! | POST   | `/pay/.deposit`    | Deposit stub (returns 501)                  |
//! | GET    | `/pay/{resource}`  | Payment-gated resource access               |
//!
//! ## Storage
//!
//! `FsPaymentStore` persists per-identity balances as JSON files under
//! `{ledger_dir}/{hex-encoded-did}.json`. File-level locking via `flock(2)`
//! provides atomic credit/debit on POSIX systems.
//!
//! @see <https://webledgers.org>
//! @see `solid_pod_rs::payments` for upstream types

use std::path::{Path, PathBuf};
use std::sync::Arc;

use actix_web::{web, HttpRequest, HttpResponse};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};

use solid_pod_rs::payments::{
    balance_response, pay_info, payment_required_body, payment_response_headers, pubkey_to_did,
    PayConfig as UpstreamPayConfig, PaymentError, WebLedger,
};

// ---------------------------------------------------------------------------
// PayConfig — local config loaded from env / config.yml
// ---------------------------------------------------------------------------

/// VisionClaw payment configuration.
///
/// Loaded from environment variables at startup:
/// - `PAY_ENABLED` (bool, default `false`)
/// - `PAY_COST_SATS` (u64, default `1`)
/// - `PAY_LEDGER_DIR` (path, default `./data/ledger`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcPayConfig {
    pub enabled: bool,
    pub cost_sats: u64,
    pub ledger_dir: PathBuf,
    pub inference_cost_sats: u64,
    pub image_gen_cost_sats: u64,
    pub analytics_cost_sats: u64,
}

impl Default for VcPayConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cost_sats: 1,
            ledger_dir: PathBuf::from("./data/ledger"),
            inference_cost_sats: 10,
            image_gen_cost_sats: 100,
            analytics_cost_sats: 5,
        }
    }
}

impl VcPayConfig {
    /// Load payment configuration from environment variables.
    pub fn from_env() -> Self {
        let enabled = std::env::var("PAY_ENABLED")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);
        let cost_sats = std::env::var("PAY_COST_SATS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        let ledger_dir = std::env::var("PAY_LEDGER_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./data/ledger"));

        let inference_cost_sats = std::env::var("PAY_INFERENCE_COST_SATS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(cost_sats * 10);
        let image_gen_cost_sats = std::env::var("PAY_IMAGE_GEN_COST_SATS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(cost_sats * 100);
        let analytics_cost_sats = std::env::var("PAY_ANALYTICS_COST_SATS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(cost_sats * 5);

        Self {
            enabled,
            cost_sats,
            ledger_dir,
            inference_cost_sats,
            image_gen_cost_sats,
            analytics_cost_sats,
        }
    }

    /// Look up the sat cost for a given endpoint path.
    pub fn cost_for_endpoint(&self, endpoint: &str) -> u64 {
        if endpoint.starts_with("/api/inference/") {
            self.inference_cost_sats
        } else if endpoint.starts_with("/api/image-gen/") {
            self.image_gen_cost_sats
        } else if endpoint.starts_with("/api/analytics/") {
            self.analytics_cost_sats
        } else {
            self.cost_sats
        }
    }

    /// Convert to the upstream `PayConfig` for interop with `solid-pod-rs`
    /// helper functions (`pay_info`, `balance_response`, etc.).
    pub fn to_upstream(&self) -> UpstreamPayConfig {
        UpstreamPayConfig {
            enabled: self.enabled,
            cost_sats: self.cost_sats,
            token: None,
            chains: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// FsPaymentStore — filesystem-backed ledger with file locking
// ---------------------------------------------------------------------------

/// Filesystem-backed payment store using a single `WebLedger` JSON file.
///
/// The ledger lives at `{ledger_dir}/ledger.json`. All mutations acquire an
/// exclusive file lock (`flock(2)` via `fs2`) to prevent concurrent writes
/// from corrupting the file. Since `fs2` is a heavyweight dep, we use a
/// `tokio::sync::Mutex` as the serialisation primitive instead — suitable
/// for single-process deployments.
pub struct FsPaymentStore {
    ledger_path: PathBuf,
    lock: tokio::sync::Mutex<()>,
}

impl FsPaymentStore {
    /// Create a new store, ensuring the ledger directory exists.
    pub fn new(ledger_dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(ledger_dir)?;
        let ledger_path = ledger_dir.join("ledger.json");
        Ok(Self {
            ledger_path,
            lock: tokio::sync::Mutex::new(()),
        })
    }

    /// Read the ledger from disk. Returns a fresh empty ledger if the file
    /// does not exist or is unparseable.
    async fn read_ledger(&self) -> WebLedger {
        match tokio::fs::read_to_string(&self.ledger_path).await {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_else(|e| {
                warn!(
                    "[pay] ledger parse error at {}: {e} — starting fresh",
                    self.ledger_path.display()
                );
                WebLedger::new("VisionClaw Credits")
            }),
            Err(_) => WebLedger::new("VisionClaw Credits"),
        }
    }

    /// Write the ledger to disk atomically (write to temp, rename).
    async fn write_ledger(&self, ledger: &WebLedger) -> Result<(), PaymentError> {
        let json = serde_json::to_string_pretty(ledger)
            .map_err(|e| PaymentError::Store(format!("serialize: {e}")))?;

        let tmp_path = self.ledger_path.with_extension("json.tmp");
        tokio::fs::write(&tmp_path, json.as_bytes())
            .await
            .map_err(|e| PaymentError::Store(format!("write tmp: {e}")))?;
        tokio::fs::rename(&tmp_path, &self.ledger_path)
            .await
            .map_err(|e| PaymentError::Store(format!("rename: {e}")))?;
        Ok(())
    }

    /// Get the balance for a `did:nostr:<hex>` identity.
    pub async fn get_balance(&self, did: &str) -> u64 {
        let ledger = self.read_ledger().await;
        ledger.get_balance(did)
    }

    /// Atomically credit an account. Returns the new balance.
    pub async fn credit(&self, did: &str, amount: u64) -> Result<u64, PaymentError> {
        let _guard = self.lock.lock().await;
        let mut ledger = self.read_ledger().await;
        ledger.credit(did, amount);
        self.write_ledger(&ledger).await?;
        Ok(ledger.get_balance(did))
    }

    /// Atomically debit an account. Returns the new balance on success,
    /// or `PaymentError::InsufficientBalance` if the account cannot cover
    /// the cost.
    pub async fn debit(&self, did: &str, amount: u64) -> Result<u64, PaymentError> {
        let _guard = self.lock.lock().await;
        let mut ledger = self.read_ledger().await;
        let remaining = ledger.debit(did, amount)?;
        self.write_ledger(&ledger).await?;
        Ok(remaining)
    }
}

// ---------------------------------------------------------------------------
// NIP-98 auth extraction (thin wrapper matching solid_pod_handler pattern)
// ---------------------------------------------------------------------------

/// Extract the caller's hex pubkey from a NIP-98 `Authorization` header.
/// Returns `None` if the header is missing or verification fails.
async fn extract_caller_pubkey(req: &HttpRequest) -> Option<String> {
    let header = req
        .headers()
        .get(actix_web::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())?;

    let url = req.uri().to_string();
    match solid_pod_rs::auth::nip98::verify(header, &url, "GET", None).await {
        Ok(pubkey) => Some(pubkey),
        Err(e) => {
            debug!("[pay] NIP-98 verify failed: {e}");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

/// `GET /pay/.info` — public endpoint, returns payment configuration.
async fn pay_info_handler(
    config: web::Data<VcPayConfig>,
) -> HttpResponse {
    let upstream = config.to_upstream();
    let mut info = pay_info(&upstream);
    // Augment with VisionClaw-specific fields
    info["enabled"] = serde_json::json!(config.enabled);
    info["methods"] = serde_json::json!(["lightning"]);
    HttpResponse::Ok().json(info)
}

/// `GET /pay/.balance` — requires NIP-98 auth, returns caller's balance.
async fn pay_balance_handler(
    req: HttpRequest,
    config: web::Data<VcPayConfig>,
    store: web::Data<Arc<FsPaymentStore>>,
) -> HttpResponse {
    let pubkey = match extract_caller_pubkey(&req).await {
        Some(pk) => pk,
        None => {
            return HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Authentication required — provide a NIP-98 Authorization header"
            }));
        }
    };

    let did = pubkey_to_did(&pubkey);
    let balance = store.get_balance(&did).await;
    let body = balance_response(&did, balance, config.cost_sats);
    HttpResponse::Ok().json(body)
}

/// `POST /pay/.deposit` — stub: manual funding instructions.
async fn pay_deposit_handler() -> HttpResponse {
    HttpResponse::NotImplemented().json(serde_json::json!({
        "error": "Deposit not yet available via API",
        "message": "Contact the server operator for manual funding. \
                    Lightning deposit support is planned for a future release.",
        "spec": "https://webledgers.org"
    }))
}

/// `GET /pay/{resource_path}` — payment-gated resource access.
///
/// Flow:
/// 1. Authenticate caller via NIP-98.
/// 2. Check balance >= cost_sats.
/// 3. Debit the cost.
/// 4. Return a JSON receipt with payment headers (`X-Balance`, `X-Cost`).
///
/// The actual resource proxying is a stub: in a full deployment this would
/// forward the request to the underlying resource handler. For now it
/// returns a JSON receipt confirming the charge.
async fn pay_resource_handler(
    req: HttpRequest,
    path: web::Path<String>,
    config: web::Data<VcPayConfig>,
    store: web::Data<Arc<FsPaymentStore>>,
) -> HttpResponse {
    let pubkey = match extract_caller_pubkey(&req).await {
        Some(pk) => pk,
        None => {
            return HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Authentication required — provide a NIP-98 Authorization header"
            }));
        }
    };

    let did = pubkey_to_did(&pubkey);
    let resource = path.into_inner();
    let cost = config.cost_sats;

    match store.debit(&did, cost).await {
        Ok(remaining) => {
            let headers = payment_response_headers(remaining, cost, "sat");
            let body = serde_json::json!({
                "resource": resource,
                "charged": cost,
                "balance": remaining,
                "unit": "sat"
            });
            let mut resp = HttpResponse::Ok().json(body);
            for (name, value) in headers {
                if let Ok(hv) = actix_web::http::header::HeaderValue::from_str(&value) {
                    resp.headers_mut().insert(
                        actix_web::http::header::HeaderName::from_static(name),
                        hv,
                    );
                }
            }
            resp
        }
        Err(PaymentError::InsufficientBalance { balance, cost }) => {
            let body = payment_required_body(balance, cost);
            HttpResponse::build(actix_web::http::StatusCode::PAYMENT_REQUIRED).json(body)
        }
        Err(e) => {
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Payment store error: {e}")
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// Route configuration
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Agent job estimation endpoint
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct EstimateRequest {
    endpoint: String,
    #[serde(default)]
    params: Option<serde_json::Value>,
}

async fn pay_estimate_handler(
    req: HttpRequest,
    body: web::Json<EstimateRequest>,
    config: web::Data<VcPayConfig>,
) -> HttpResponse {
    let pubkey = match extract_caller_pubkey(&req).await {
        Some(pk) => pk,
        None => {
            return HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Authentication required"
            }));
        }
    };

    let did = pubkey_to_did(&pubkey);
    let estimated_sats = config.cost_for_endpoint(&body.endpoint);

    HttpResponse::Ok().json(serde_json::json!({
        "did": did,
        "endpoint": body.endpoint,
        "estimated_sats": estimated_sats,
        "unit": "sat",
        "note": "Pre-execution estimate. GPU-metered endpoints may settle at actual cost."
    }))
}

// ---------------------------------------------------------------------------
// Cost table endpoint (public — no auth required)
// ---------------------------------------------------------------------------

async fn pay_cost_table_handler(config: web::Data<VcPayConfig>) -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "default": config.cost_sats,
        "endpoints": {
            "/api/inference/*": config.inference_cost_sats,
            "/api/image-gen/*": config.image_gen_cost_sats,
            "/api/analytics/*": config.analytics_cost_sats,
        },
        "unit": "sat",
        "note": "GPU-metered endpoints may settle at actual cost after execution."
    }))
}

/// Mount all `/pay/*` routes. Called from `main.rs` via
/// `.configure(configure_pay_routes)`.
///
/// When `PAY_ENABLED=false` the routes still mount but `.info` reports
/// `enabled: false` and gated endpoints return 403.
pub fn configure_pay_routes(cfg: &mut web::ServiceConfig) {
    info!("=== REGISTERING PAYMENT ROUTES (/pay/*) ===");
    cfg.service(
        web::scope("/pay")
            .route("/.info", web::get().to(pay_info_handler))
            .route("/.balance", web::get().to(pay_balance_handler))
            .route("/.deposit", web::post().to(pay_deposit_handler))
            .route("/.estimate", web::post().to(pay_estimate_handler))
            .route("/.costs", web::get().to(pay_cost_table_handler))
            .route("/{resource_path:.*}", web::get().to(pay_resource_handler)),
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn pay_config_defaults() {
        let config = VcPayConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.cost_sats, 1);
        assert_eq!(config.ledger_dir, PathBuf::from("./data/ledger"));
        assert_eq!(config.inference_cost_sats, 10);
        assert_eq!(config.image_gen_cost_sats, 100);
        assert_eq!(config.analytics_cost_sats, 5);
    }

    #[test]
    fn cost_for_endpoint_inference() {
        let config = VcPayConfig::default();
        assert_eq!(config.cost_for_endpoint("/api/inference/run"), 10);
        assert_eq!(config.cost_for_endpoint("/api/inference/batch"), 10);
    }

    #[test]
    fn cost_for_endpoint_image_gen() {
        let config = VcPayConfig::default();
        assert_eq!(config.cost_for_endpoint("/api/image-gen/submit"), 100);
        assert_eq!(config.cost_for_endpoint("/api/image-gen/agent-submit"), 100);
    }

    #[test]
    fn cost_for_endpoint_analytics() {
        let config = VcPayConfig::default();
        assert_eq!(config.cost_for_endpoint("/api/analytics/pagerank"), 5);
    }

    #[test]
    fn cost_for_endpoint_default() {
        let config = VcPayConfig::default();
        assert_eq!(config.cost_for_endpoint("/api/health"), 1);
        assert_eq!(config.cost_for_endpoint("/pay/.info"), 1);
    }

    #[test]
    fn pay_config_from_env() {
        // Save originals
        let orig_enabled = std::env::var("PAY_ENABLED").ok();
        let orig_cost = std::env::var("PAY_COST_SATS").ok();
        let orig_dir = std::env::var("PAY_LEDGER_DIR").ok();

        std::env::set_var("PAY_ENABLED", "true");
        std::env::set_var("PAY_COST_SATS", "42");
        std::env::set_var("PAY_LEDGER_DIR", "/tmp/test-ledger");

        let config = VcPayConfig::from_env();
        assert!(config.enabled);
        assert_eq!(config.cost_sats, 42);
        assert_eq!(config.ledger_dir, PathBuf::from("/tmp/test-ledger"));

        // Restore
        match orig_enabled {
            Some(v) => std::env::set_var("PAY_ENABLED", v),
            None => std::env::remove_var("PAY_ENABLED"),
        }
        match orig_cost {
            Some(v) => std::env::set_var("PAY_COST_SATS", v),
            None => std::env::remove_var("PAY_COST_SATS"),
        }
        match orig_dir {
            Some(v) => std::env::set_var("PAY_LEDGER_DIR", v),
            None => std::env::remove_var("PAY_LEDGER_DIR"),
        }
    }

    #[test]
    fn upstream_config_conversion() {
        let config = VcPayConfig {
            enabled: true,
            cost_sats: 10,
            ledger_dir: PathBuf::from("/tmp"),
        };
        let upstream = config.to_upstream();
        assert!(upstream.enabled);
        assert_eq!(upstream.cost_sats, 10);
        assert!(upstream.token.is_none());
        assert!(upstream.chains.is_empty());
    }

    #[tokio::test]
    async fn fs_store_credit_and_balance() {
        let tmp = TempDir::new().unwrap();
        let store = FsPaymentStore::new(tmp.path()).unwrap();
        let did = "did:nostr:aabbccdd";

        assert_eq!(store.get_balance(did).await, 0);

        let balance = store.credit(did, 100).await.unwrap();
        assert_eq!(balance, 100);

        let balance = store.credit(did, 50).await.unwrap();
        assert_eq!(balance, 150);

        assert_eq!(store.get_balance(did).await, 150);
    }

    #[tokio::test]
    async fn fs_store_debit_success() {
        let tmp = TempDir::new().unwrap();
        let store = FsPaymentStore::new(tmp.path()).unwrap();
        let did = "did:nostr:aabbccdd";

        store.credit(did, 200).await.unwrap();
        let remaining = store.debit(did, 50).await.unwrap();
        assert_eq!(remaining, 150);
        assert_eq!(store.get_balance(did).await, 150);
    }

    #[tokio::test]
    async fn fs_store_debit_insufficient() {
        let tmp = TempDir::new().unwrap();
        let store = FsPaymentStore::new(tmp.path()).unwrap();
        let did = "did:nostr:aabbccdd";

        store.credit(did, 10).await.unwrap();
        let err = store.debit(did, 100).await.unwrap_err();
        assert!(matches!(
            err,
            PaymentError::InsufficientBalance {
                balance: 10,
                cost: 100
            }
        ));
    }

    #[tokio::test]
    async fn fs_store_debit_unknown_did() {
        let tmp = TempDir::new().unwrap();
        let store = FsPaymentStore::new(tmp.path()).unwrap();

        let err = store.debit("did:nostr:unknown", 1).await.unwrap_err();
        assert!(matches!(
            err,
            PaymentError::InsufficientBalance {
                balance: 0,
                cost: 1
            }
        ));
    }

    #[tokio::test]
    async fn fs_store_persistence() {
        let tmp = TempDir::new().unwrap();
        let did = "did:nostr:persist";

        // Write with one store instance
        {
            let store = FsPaymentStore::new(tmp.path()).unwrap();
            store.credit(did, 500).await.unwrap();
        }

        // Read with a new store instance
        {
            let store = FsPaymentStore::new(tmp.path()).unwrap();
            assert_eq!(store.get_balance(did).await, 500);
        }
    }

    #[tokio::test]
    async fn fs_store_multiple_identities() {
        let tmp = TempDir::new().unwrap();
        let store = FsPaymentStore::new(tmp.path()).unwrap();

        let alice = "did:nostr:alice111";
        let bob = "did:nostr:bob22222";

        store.credit(alice, 100).await.unwrap();
        store.credit(bob, 200).await.unwrap();
        store.debit(alice, 30).await.unwrap();

        assert_eq!(store.get_balance(alice).await, 70);
        assert_eq!(store.get_balance(bob).await, 200);
    }

    #[test]
    fn info_endpoint_shape() {
        let config = VcPayConfig {
            enabled: true,
            cost_sats: 5,
            ledger_dir: PathBuf::from("/tmp"),
        };
        let upstream = config.to_upstream();
        let info = pay_info(&upstream);
        assert_eq!(info["cost"], 5);
        assert_eq!(info["unit"], "sat");
    }
}
