//! vc-cli — VisionClaw power-user operations CLI.
//!
//! Currently implements a single sub-command:
//!
//!   vc-cli bootstrap-power-user --env PATH [--pubkey HEX] [--pod-url URL]
//!                               [--dry-run] [--force]
//!
//! Reads a .env file, extracts the GitHub credentials the power user already
//! carries in their local environment, and writes them to their Solid-style
//! Pod at `./private/config/github` using a NIP-98 signed PUT.
//!
//! Design notes:
//!
//!  * All I/O is async under tokio; the binary is self-contained and does not
//!    require any other webxr services to be running.
//!  * The GitHub token is treated as high-sensitivity: it is held in a
//!    `Zeroizing<String>` wrapper, never logged in clear, and the constructed
//!    JSON payload is *not* echoed to stderr on error — only the error kind
//!    and the target URL.
//!  * Signing supports two paths, matching the spec:
//!      - SERVER_NOSTR_PRIVKEY (hex)     — server acts on the user's behalf
//!      - POWER_USER_NSEC      (bech32)  — the user signs once to bootstrap
//!    Exactly one of the two is required unless `--dry-run` is passed.

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use reqwest::StatusCode;
use serde::Serialize;
use zeroize::Zeroizing;

use webxr::utils::nip98::{build_auth_header, generate_nip98_token, Nip98Config};

// ---------------------------------------------------------------------------
// CLI definitions
// ---------------------------------------------------------------------------

#[derive(Debug, Parser)]
#[command(
    name = "vc-cli",
    version,
    about = "VisionClaw operations CLI",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Seed a power-user Pod with GitHub credentials from a local .env file.
    BootstrapPowerUser(BootstrapArgs),
}

#[derive(Debug, clap::Args)]
struct BootstrapArgs {
    /// Path to a .env file containing GITHUB_OWNER/REPO/BRANCH/BASE_PATH/TOKEN.
    #[arg(long, value_name = "PATH")]
    env: PathBuf,

    /// Owner (power-user) Nostr pubkey in hex. Defaults to $POWER_USER_PUBKEY.
    #[arg(long, value_name = "HEX")]
    pubkey: Option<String>,

    /// Full Pod base URL. Defaults to ${POD_BASE_URL}/{pubkey}.
    #[arg(long, value_name = "URL")]
    pod_url: Option<String>,

    /// Print the payload and target URL but do not make the HTTP request.
    #[arg(long, default_value_t = false)]
    dry_run: bool,

    /// Overwrite an existing config without prompting.
    #[arg(long, default_value_t = false)]
    force: bool,
}

// ---------------------------------------------------------------------------
// Library entry point (so tests can exercise the full flow).
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> ExitCode {
    // Plain stderr logger; do not forward to anything that could leak tokens.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    let result = match cli.command {
        Command::BootstrapPowerUser(args) => run_bootstrap(&args).await,
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // IMPORTANT: print only the error message.  We never echo the
            // payload here — it would contain the GitHub token.
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors produced by the bootstrap flow.  Variants hold only non-sensitive
/// information — no token, no payload body.
#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    #[error("failed to open .env file '{path}': {source}")]
    EnvOpen {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read .env file '{path}': {source}")]
    EnvRead {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("missing required key(s) in .env: {0}")]
    MissingKeys(String),
    #[error("no owner pubkey provided (pass --pubkey HEX or set POWER_USER_PUBKEY)")]
    MissingPubkey,
    #[error(
        "invalid owner pubkey '{0}' — must be 64 lowercase hex characters (Nostr x-only pubkey)"
    )]
    InvalidPubkey(String),
    #[error(
        "no Pod URL — pass --pod-url URL or set POD_BASE_URL (used as {{POD_BASE_URL}}/{{pubkey}})"
    )]
    MissingPodUrl,
    #[error(
        "no signing key available — set SERVER_NOSTR_PRIVKEY (hex) or POWER_USER_NSEC (bech32)"
    )]
    MissingSigningKey,
    #[error("invalid SERVER_NOSTR_PRIVKEY (expected 64 hex chars): {0}")]
    InvalidServerKey(String),
    #[error("invalid POWER_USER_NSEC (expected Nostr nsec1... bech32): {0}")]
    InvalidNsec(String),
    #[error("failed to sign NIP-98 event: {0}")]
    Signing(String),
    #[error("aborted: file exists at {0}/private/config/github")]
    Aborted(String),
    #[error("HTTP request failed: {0}")]
    Http(String),
    #[error("Pod returned HTTP {status} at {url}")]
    PodError { status: StatusCode, url: String },
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Parsed and validated inputs.  `token` is wrapped in `Zeroizing` so that
/// its backing memory is overwritten when it is dropped.
pub struct BootstrapInputs {
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub base_paths: Vec<String>,
    pub token: Zeroizing<String>,
}

/// JSON payload that is written to ./private/config/github.
///
/// We implement `Serialize` manually-derived here; the token is written to
/// the wire but never logged to stderr.  Callers that want to inspect the
/// payload for debugging (e.g. `--dry-run`) use [`PayloadRedacted`] instead.
#[derive(Debug, Serialize)]
pub struct GithubConfigPayload<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    pub branch: &'a str,
    pub base_paths: &'a [String],
    pub token: &'a str,
    pub token_storage: &'a str,
    pub created_at: String,
    pub created_by: &'a str,
}

/// Redacted view of the payload — safe to print.
#[derive(Debug, Serialize)]
pub struct PayloadRedacted<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    pub branch: &'a str,
    pub base_paths: &'a [String],
    pub token: String,
    pub token_storage: &'a str,
    pub created_at: &'a str,
    pub created_by: &'a str,
}

// ---------------------------------------------------------------------------
// Top-level run function
// ---------------------------------------------------------------------------

async fn run_bootstrap(args: &BootstrapArgs) -> Result<(), BootstrapError> {
    // 1. Read the .env file
    let raw = read_env_file(&args.env)?;

    // 2. Extract required keys
    let inputs = parse_bootstrap_inputs(&raw)?;

    // 3. Determine owner pubkey
    let pubkey = resolve_pubkey(args.pubkey.as_deref())?;

    // 4. Determine Pod URL
    let pod_url = resolve_pod_url(args.pod_url.as_deref(), &pubkey)?;

    // 5. Build payload JSON
    let created_at = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let payload = GithubConfigPayload {
        owner: &inputs.owner,
        repo: &inputs.repo,
        branch: &inputs.branch,
        base_paths: &inputs.base_paths,
        token: inputs.token.as_str(),
        token_storage: "plain",
        created_at: created_at.clone(),
        created_by: "vc bootstrap-power-user",
    };

    let body_json = serde_json::to_string(&payload)
        .map_err(|e| BootstrapError::Http(format!("serialize payload: {e}")))?;

    let target_url = format!("{}/private/config/github", pod_url.trim_end_matches('/'));

    // 6. Dry-run short-circuit
    if args.dry_run {
        let redacted = PayloadRedacted {
            owner: &inputs.owner,
            repo: &inputs.repo,
            branch: &inputs.branch,
            base_paths: &inputs.base_paths,
            token: mask_token(inputs.token.as_str()),
            token_storage: "plain",
            created_at: &created_at,
            created_by: "vc bootstrap-power-user",
        };
        let pretty = serde_json::to_string_pretty(&redacted)
            .map_err(|e| BootstrapError::Http(format!("serialize dry-run: {e}")))?;
        println!("[dry-run] target: PUT {target_url}");
        println!("[dry-run] payload (token redacted):\n{pretty}");
        return Ok(());
    }

    // 7. Idempotency — prompt unless --force.
    //
    //    We cannot safely "check existence" without also sending a signed
    //    GET (which duplicates the auth logic and offers little over just
    //    attempting the PUT and reporting any conflict the Pod raises).
    //    Instead we warn the user interactively before performing the
    //    destructive operation, matching the spec's `--force` behaviour.
    if !args.force && std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        eprint!(
            "About to PUT {}/private/config/github. Continue? [y/N] ",
            pod_url
        );
        std::io::stderr().flush().ok();
        let mut answer = String::new();
        std::io::stdin()
            .read_line(&mut answer)
            .map_err(|e| BootstrapError::Http(format!("read stdin: {e}")))?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            return Err(BootstrapError::Aborted(pod_url));
        }
    }

    // 8. Sign + HTTP PUT
    let token_b64 = sign_request(&target_url, "PUT", &body_json)?;
    let auth = build_auth_header(&token_b64);

    let client = reqwest::Client::builder()
        .user_agent(concat!("vc-cli/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| BootstrapError::Http(e.to_string()))?;

    let resp = client
        .put(&target_url)
        .header("Authorization", auth)
        .header("Content-Type", "application/json")
        .body(body_json)
        .send()
        .await
        .map_err(|e| BootstrapError::Http(e.to_string()))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(BootstrapError::PodError {
            status,
            url: target_url,
        });
    }

    // 9. Success — single-line confirmation, no token anywhere.
    println!("✓ Wrote {target_url}");
    Ok(())
}

// ---------------------------------------------------------------------------
// .env parsing
// ---------------------------------------------------------------------------

/// Read a .env file into a Vec<(key, value)> preserving original order.
/// Ignores blank lines, comments (`#` lines) and lines without `=`.
/// Strips surrounding double/single quotes on values.
pub fn read_env_file(path: &Path) -> Result<Vec<(String, String)>, BootstrapError> {
    let file = File::open(path).map_err(|e| BootstrapError::EnvOpen {
        path: path.display().to_string(),
        source: e,
    })?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| BootstrapError::EnvRead {
            path: path.display().to_string(),
            source: e,
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Drop an optional `export ` prefix.
        let trimmed = trimmed.strip_prefix("export ").unwrap_or(trimmed);
        if let Some((k, v)) = trimmed.split_once('=') {
            let key = k.trim().to_string();
            if key.is_empty() {
                continue;
            }
            let raw = v.trim();
            let value = unquote(raw);
            out.push((key, value));
        }
    }
    Ok(out)
}

fn unquote(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// Collect and validate the bootstrap inputs from a parsed .env list.
///
/// `GITHUB_BASE_PATH` may be specified either:
///   * as a single comma-separated value  (GITHUB_BASE_PATH=a/pages,b/pages)
///   * as multiple duplicate lines        (GITHUB_BASE_PATH=a/pages then b/pages)
///
/// Both forms produce the same `base_paths` Vec.
pub fn parse_bootstrap_inputs(
    entries: &[(String, String)],
) -> Result<BootstrapInputs, BootstrapError> {
    let mut owner: Option<String> = None;
    let mut repo: Option<String> = None;
    let mut branch: Option<String> = None;
    let mut base_paths: Vec<String> = Vec::new();
    let mut token: Option<String> = None;

    for (k, v) in entries {
        match k.as_str() {
            "GITHUB_OWNER" => owner = Some(v.clone()),
            "GITHUB_REPO" => repo = Some(v.clone()),
            "GITHUB_BRANCH" => branch = Some(v.clone()),
            "GITHUB_BASE_PATH" => {
                // Accept both comma-separated and duplicated.
                for part in v.split(',') {
                    let p = part.trim().trim_end_matches('/');
                    if !p.is_empty() {
                        base_paths.push(p.to_string());
                    }
                }
            }
            "GITHUB_TOKEN" => token = Some(v.clone()),
            _ => {}
        }
    }

    // Deduplicate base_paths while preserving order.
    let mut seen = std::collections::BTreeSet::new();
    base_paths.retain(|p| seen.insert(p.clone()));

    let mut missing: Vec<&'static str> = Vec::new();
    if owner.as_deref().map(str::is_empty).unwrap_or(true) {
        missing.push("GITHUB_OWNER");
    }
    if repo.as_deref().map(str::is_empty).unwrap_or(true) {
        missing.push("GITHUB_REPO");
    }
    if branch.as_deref().map(str::is_empty).unwrap_or(true) {
        missing.push("GITHUB_BRANCH");
    }
    if base_paths.is_empty() {
        missing.push("GITHUB_BASE_PATH");
    }
    if token.as_deref().map(str::is_empty).unwrap_or(true) {
        missing.push("GITHUB_TOKEN");
    }
    if !missing.is_empty() {
        return Err(BootstrapError::MissingKeys(missing.join(", ")));
    }

    Ok(BootstrapInputs {
        owner: owner.unwrap(),
        repo: repo.unwrap(),
        branch: branch.unwrap(),
        base_paths,
        token: Zeroizing::new(token.unwrap()),
    })
}

// ---------------------------------------------------------------------------
// Pubkey / URL resolution
// ---------------------------------------------------------------------------

fn resolve_pubkey(flag: Option<&str>) -> Result<String, BootstrapError> {
    let candidate = match flag {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => match std::env::var("POWER_USER_PUBKEY") {
            Ok(v) if !v.is_empty() => v,
            _ => return Err(BootstrapError::MissingPubkey),
        },
    };
    if !is_valid_hex_pubkey(&candidate) {
        return Err(BootstrapError::InvalidPubkey(candidate));
    }
    Ok(candidate.to_ascii_lowercase())
}

fn is_valid_hex_pubkey(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn resolve_pod_url(flag: Option<&str>, pubkey: &str) -> Result<String, BootstrapError> {
    if let Some(v) = flag {
        if !v.is_empty() {
            return Ok(v.trim_end_matches('/').to_string());
        }
    }
    match std::env::var("POD_BASE_URL") {
        Ok(base) if !base.is_empty() => {
            Ok(format!("{}/{}", base.trim_end_matches('/'), pubkey))
        }
        _ => Err(BootstrapError::MissingPodUrl),
    }
}

// ---------------------------------------------------------------------------
// NIP-98 signing
// ---------------------------------------------------------------------------

/// Sign an outgoing request with either SERVER_NOSTR_PRIVKEY (hex) or
/// POWER_USER_NSEC (bech32).  SERVER_NOSTR_PRIVKEY wins if both are set.
fn sign_request(url: &str, method: &str, body: &str) -> Result<String, BootstrapError> {
    use nostr_sdk::prelude::*;

    let keys = if let Ok(hex) = std::env::var("SERVER_NOSTR_PRIVKEY") {
        if hex.is_empty() {
            // fall through to nsec path
            load_nsec_keys()?
        } else {
            let sk = Zeroizing::new(hex.trim().to_string());
            let secret_key = SecretKey::from_hex(sk.as_str())
                .map_err(|e| BootstrapError::InvalidServerKey(e.to_string()))?;
            Keys::new(secret_key)
        }
    } else {
        load_nsec_keys()?
    };

    let config = Nip98Config {
        url: url.to_string(),
        method: method.to_string(),
        body: Some(body.to_string()),
    };
    generate_nip98_token(&keys, &config).map_err(|e| BootstrapError::Signing(e.to_string()))
}

fn load_nsec_keys() -> Result<nostr_sdk::prelude::Keys, BootstrapError> {
    use nostr_sdk::prelude::*;
    let nsec = std::env::var("POWER_USER_NSEC").map_err(|_| BootstrapError::MissingSigningKey)?;
    if nsec.is_empty() {
        return Err(BootstrapError::MissingSigningKey);
    }
    let nsec = Zeroizing::new(nsec.trim().to_string());
    let secret_key = SecretKey::parse(nsec.as_str())
        .map_err(|e| BootstrapError::InvalidNsec(e.to_string()))?;
    Ok(Keys::new(secret_key))
}

// ---------------------------------------------------------------------------
// Token masking
// ---------------------------------------------------------------------------

/// Mask a secret token for printable output.
///
/// For a token like `ghp_ABCDE...WXYZ` returns `ghp_****WXYZ`.
/// Short tokens (<8 chars) are fully redacted.
pub fn mask_token(token: &str) -> String {
    let len = token.chars().count();
    if len < 8 {
        return "****".to_string();
    }
    let first: String = token.chars().take(4).collect();
    let last: String = token.chars().skip(len - 4).collect();
    format!("{first}****{last}")
}

// ---------------------------------------------------------------------------
// Tests (integration tests live in tests/bootstrap_cli.rs)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_token_to_first4_last4() {
        assert_eq!(mask_token("ghp_abcdefghijklmnop1234"), "ghp_****1234");
        assert_eq!(mask_token("1234567890"), "1234****7890");
        assert_eq!(mask_token("short"), "****");
    }

    #[test]
    fn unquote_strips_matching_quotes() {
        assert_eq!(unquote("\"hello\""), "hello");
        assert_eq!(unquote("'hello'"), "hello");
        assert_eq!(unquote("no quotes"), "no quotes");
        assert_eq!(unquote("\"mixed'"), "\"mixed'");
    }

    #[test]
    fn hex_pubkey_validation() {
        assert!(is_valid_hex_pubkey(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ));
        assert!(!is_valid_hex_pubkey("too_short"));
        assert!(!is_valid_hex_pubkey(
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"
        ));
    }
}
