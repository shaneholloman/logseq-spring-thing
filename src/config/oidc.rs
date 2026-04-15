//! OIDC Configuration Types (ADR-040)
//!
//! Configuration for OpenID Connect enterprise identity integration.
//! When `enabled` is false (the default), the system operates in Nostr-only mode.
//! When enabled, OIDC provides the primary authentication path for enterprise
//! users, with server-side ephemeral Nostr keypairs for provenance signing.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level OIDC configuration.
///
/// Loaded from environment variables or the settings repository.
/// All fields are optional except `enabled` so the system can start
/// in Nostr-only mode without any OIDC configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcConfig {
    /// Whether OIDC authentication is enabled. Default: false (Nostr-only).
    #[serde(default)]
    pub enabled: bool,

    /// OIDC discovery URL (e.g., `https://login.microsoftonline.com/{tenant}/v2.0`).
    /// Required when `enabled` is true.
    #[serde(default)]
    pub issuer_url: Option<String>,

    /// Client ID registered with the OIDC provider.
    #[serde(default)]
    pub client_id: Option<String>,

    /// Client secret (confidential client flow). Stored in environment, never in config files.
    #[serde(default, skip_serializing)]
    pub client_secret: Option<String>,

    /// Redirect URI after OIDC authentication callback.
    #[serde(default)]
    pub redirect_uri: Option<String>,

    /// Scopes to request during OIDC authorization. Default: `["openid", "profile", "email"]`.
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,

    /// Claim name in the ID token that contains role information.
    /// Default: `"roles"`. Common alternatives: `"groups"`, `"realm_access.roles"`.
    #[serde(default = "default_role_claim")]
    pub role_claim: String,

    /// Mapping from OIDC role/group claim values to VisionClaw `EnterpriseRole` names.
    ///
    /// Keys are the claim values from the IdP (e.g., `"visionclaw-brokers"`).
    /// Values are the snake_case role names: `"broker"`, `"admin"`, `"auditor"`, `"contributor"`.
    ///
    /// Users whose tokens contain none of these mapped values default to `Contributor`.
    #[serde(default = "default_role_mapping")]
    pub role_mapping: HashMap<String, String>,

    /// Token validation settings.
    #[serde(default)]
    pub token: TokenValidationConfig,

    /// Ephemeral Nostr keypair configuration (ADR-040 dual-stack).
    #[serde(default)]
    pub ephemeral_keys: EphemeralKeyConfig,
}

/// Settings governing JWT / ID token validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenValidationConfig {
    /// Allowed clock skew in seconds for token expiry validation.
    #[serde(default = "default_clock_skew")]
    pub clock_skew_seconds: u64,

    /// Expected audience (`aud` claim). Typically the same as `client_id`.
    #[serde(default)]
    pub expected_audience: Option<String>,

    /// JWKS refresh interval in seconds. Default: 3600 (1 hour).
    #[serde(default = "default_jwks_refresh")]
    pub jwks_refresh_seconds: u64,
}

/// Configuration for server-side ephemeral Nostr keypair management (ADR-040).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EphemeralKeyConfig {
    /// Where to store ephemeral keypairs: `"solid_pod"` or `"database"`.
    #[serde(default = "default_key_storage")]
    pub storage: String,

    /// Key rotation interval in days. Default: 30.
    #[serde(default = "default_rotation_days")]
    pub rotation_days: u32,

    /// Whether to archive rotated keys for provenance verification.
    #[serde(default = "default_true")]
    pub archive_rotated: bool,
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

fn default_scopes() -> Vec<String> {
    vec![
        "openid".to_string(),
        "profile".to_string(),
        "email".to_string(),
    ]
}

fn default_role_claim() -> String {
    "roles".to_string()
}

fn default_role_mapping() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("broker".to_string(), "broker".to_string());
    m.insert("admin".to_string(), "admin".to_string());
    m.insert("auditor".to_string(), "auditor".to_string());
    m.insert("contributor".to_string(), "contributor".to_string());
    m
}

fn default_clock_skew() -> u64 {
    120
}

fn default_jwks_refresh() -> u64 {
    3600
}

fn default_key_storage() -> String {
    "database".to_string()
}

fn default_rotation_days() -> u32 {
    30
}

fn default_true() -> bool {
    true
}

impl Default for OidcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            issuer_url: None,
            client_id: None,
            client_secret: None,
            redirect_uri: None,
            scopes: default_scopes(),
            role_claim: default_role_claim(),
            role_mapping: default_role_mapping(),
            token: TokenValidationConfig::default(),
            ephemeral_keys: EphemeralKeyConfig::default(),
        }
    }
}

impl Default for TokenValidationConfig {
    fn default() -> Self {
        Self {
            clock_skew_seconds: default_clock_skew(),
            expected_audience: None,
            jwks_refresh_seconds: default_jwks_refresh(),
        }
    }
}

impl Default for EphemeralKeyConfig {
    fn default() -> Self {
        Self {
            storage: default_key_storage(),
            rotation_days: default_rotation_days(),
            archive_rotated: true,
        }
    }
}

impl OidcConfig {
    /// Validates that mandatory fields are present when OIDC is enabled.
    /// Returns a list of missing field names.
    pub fn validate(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.enabled {
            if self.issuer_url.is_none() {
                missing.push("issuer_url");
            }
            if self.client_id.is_none() {
                missing.push("client_id");
            }
            if self.redirect_uri.is_none() {
                missing.push("redirect_uri");
            }
        }
        missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_disabled() {
        let cfg = OidcConfig::default();
        assert!(!cfg.enabled);
        assert!(cfg.issuer_url.is_none());
        assert_eq!(cfg.role_claim, "roles");
        assert_eq!(cfg.scopes.len(), 3);
    }

    #[test]
    fn test_validation_passes_when_disabled() {
        let cfg = OidcConfig::default();
        assert!(cfg.validate().is_empty());
    }

    #[test]
    fn test_validation_fails_when_enabled_without_fields() {
        let cfg = OidcConfig {
            enabled: true,
            ..Default::default()
        };
        let missing = cfg.validate();
        assert!(missing.contains(&"issuer_url"));
        assert!(missing.contains(&"client_id"));
        assert!(missing.contains(&"redirect_uri"));
    }

    #[test]
    fn test_validation_passes_when_enabled_with_fields() {
        let cfg = OidcConfig {
            enabled: true,
            issuer_url: Some("https://login.example.com/v2.0".to_string()),
            client_id: Some("abc-123".to_string()),
            redirect_uri: Some("https://app.example.com/callback".to_string()),
            ..Default::default()
        };
        assert!(cfg.validate().is_empty());
    }

    #[test]
    fn test_role_mapping_defaults() {
        let cfg = OidcConfig::default();
        assert_eq!(cfg.role_mapping.get("broker"), Some(&"broker".to_string()));
        assert_eq!(cfg.role_mapping.get("admin"), Some(&"admin".to_string()));
        assert_eq!(cfg.role_mapping.get("auditor"), Some(&"auditor".to_string()));
    }

    #[test]
    fn test_client_secret_not_serialized() {
        let cfg = OidcConfig {
            client_secret: Some("super-secret".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(!json.contains("super-secret"));
    }

    #[test]
    fn test_ephemeral_key_defaults() {
        let cfg = EphemeralKeyConfig::default();
        assert_eq!(cfg.storage, "database");
        assert_eq!(cfg.rotation_days, 30);
        assert!(cfg.archive_rotated);
    }
}
