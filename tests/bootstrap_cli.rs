//! Integration tests for the `vc-cli bootstrap-power-user` subcommand.
//!
//! These tests drive the compiled binary via `std::process::Command` so that
//! the full clap/stdin/exit-code surface is exercised.  HTTP interactions use
//! `wiremock` to stand up an in-process Pod.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use tempfile::NamedTempFile;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Path to the built `vc-cli` binary produced by `cargo test`.
fn cli_bin() -> PathBuf {
    // Use the cargo-provided env var set when building integration tests.
    // CARGO_BIN_EXE_<name> always points at the test-matching artifact.
    PathBuf::from(env!("CARGO_BIN_EXE_vc-cli"))
}

fn write_env(contents: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().expect("tempfile");
    f.write_all(contents.as_bytes()).expect("write");
    f.flush().expect("flush");
    f
}

/// A 64-char hex Nostr pubkey — deterministic so tests are stable.
const TEST_PUBKEY: &str =
    "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

/// A 64-char hex secret key for NIP-98 signing.  NOT a real key.
const TEST_SERVER_SK: &str =
    "0000000000000000000000000000000000000000000000000000000000000001";

const GOOD_ENV: &str = "\
# Example comment
GITHUB_OWNER=jjohare
GITHUB_REPO=logseq
GITHUB_BRANCH=main
GITHUB_BASE_PATH=mainKnowledgeGraph/pages,workingGraph/pages
GITHUB_TOKEN=ghp_abcdefghijklmnop1234
";

// ---------------------------------------------------------------------------
// Env parsing — unit-style coverage of the binary's public helpers.
// ---------------------------------------------------------------------------

// We can't import the bin crate directly; instead we verify parsing by
// driving the CLI with --dry-run (which prints the parsed payload).

#[test]
fn dry_run_produces_expected_payload() {
    let env_file = write_env(GOOD_ENV);
    let out = Command::new(cli_bin())
        .args([
            "bootstrap-power-user",
            "--env",
            env_file.path().to_str().unwrap(),
            "--pubkey",
            TEST_PUBKEY,
            "--pod-url",
            "https://pods.example.org/foo",
            "--dry-run",
        ])
        .output()
        .expect("run cli");

    assert!(
        out.status.success(),
        "dry-run exited non-zero: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        stdout.contains("PUT https://pods.example.org/foo/private/config/github"),
        "stdout missing target line: {stdout}"
    );
    assert!(stdout.contains("\"owner\": \"jjohare\""), "stdout = {stdout}");
    assert!(stdout.contains("\"repo\": \"logseq\""), "stdout = {stdout}");
    assert!(stdout.contains("\"branch\": \"main\""), "stdout = {stdout}");
    assert!(
        stdout.contains("mainKnowledgeGraph/pages"),
        "stdout = {stdout}"
    );
    assert!(stdout.contains("workingGraph/pages"), "stdout = {stdout}");
    // Token must be masked in dry-run output.
    assert!(stdout.contains("ghp_****1234"), "stdout = {stdout}");
    assert!(
        !stdout.contains("ghp_abcdefghijklmnop1234"),
        "dry-run leaked raw token: {stdout}"
    );
}

#[test]
fn dry_run_accepts_multiline_base_path() {
    let env = "\
GITHUB_OWNER=jjohare
GITHUB_REPO=logseq
GITHUB_BRANCH=main
GITHUB_BASE_PATH=mainKnowledgeGraph/pages
GITHUB_BASE_PATH=workingGraph/pages
GITHUB_TOKEN=ghp_abcdefghijklmnop1234
";
    let env_file = write_env(env);
    let out = Command::new(cli_bin())
        .args([
            "bootstrap-power-user",
            "--env",
            env_file.path().to_str().unwrap(),
            "--pubkey",
            TEST_PUBKEY,
            "--pod-url",
            "https://pods.example.org/foo",
            "--dry-run",
        ])
        .output()
        .expect("run cli");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("mainKnowledgeGraph/pages"));
    assert!(stdout.contains("workingGraph/pages"));
}

#[test]
fn missing_required_keys_errors_clearly() {
    // Omit TOKEN and BRANCH.
    let env = "\
GITHUB_OWNER=jjohare
GITHUB_REPO=logseq
GITHUB_BASE_PATH=a/pages
";
    let env_file = write_env(env);
    let out = Command::new(cli_bin())
        .args([
            "bootstrap-power-user",
            "--env",
            env_file.path().to_str().unwrap(),
            "--pubkey",
            TEST_PUBKEY,
            "--pod-url",
            "https://pods.example.org/foo",
            "--dry-run",
        ])
        .output()
        .expect("run cli");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("missing required"), "stderr={stderr}");
    assert!(stderr.contains("GITHUB_BRANCH"), "stderr={stderr}");
    assert!(stderr.contains("GITHUB_TOKEN"), "stderr={stderr}");
}

#[test]
fn missing_pubkey_errors_clearly() {
    let env_file = write_env(GOOD_ENV);
    // Scrub any ambient POWER_USER_PUBKEY / pubkey flag.
    let out = Command::new(cli_bin())
        .args([
            "bootstrap-power-user",
            "--env",
            env_file.path().to_str().unwrap(),
            "--pod-url",
            "https://pods.example.org/foo",
            "--dry-run",
        ])
        .env_remove("POWER_USER_PUBKEY")
        .output()
        .expect("run cli");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no owner pubkey"),
        "stderr should mention missing pubkey, got: {stderr}"
    );
}

#[test]
fn invalid_pubkey_errors_clearly() {
    let env_file = write_env(GOOD_ENV);
    let out = Command::new(cli_bin())
        .args([
            "bootstrap-power-user",
            "--env",
            env_file.path().to_str().unwrap(),
            "--pubkey",
            "not-a-hex-pubkey",
            "--pod-url",
            "https://pods.example.org/foo",
            "--dry-run",
        ])
        .output()
        .expect("run cli");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("invalid owner pubkey"),
        "stderr={stderr}"
    );
}

#[test]
fn missing_env_file_errors_clearly() {
    let out = Command::new(cli_bin())
        .args([
            "bootstrap-power-user",
            "--env",
            "/no/such/path/.env",
            "--pubkey",
            TEST_PUBKEY,
            "--pod-url",
            "https://pods.example.org/foo",
            "--dry-run",
        ])
        .output()
        .expect("run cli");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("failed to open .env file"));
}

// ---------------------------------------------------------------------------
// HTTP round-trip tests (mock Pod)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn put_succeeds_with_server_nostr_privkey() {
    let server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/private/config/github"))
        .and(header_matches("authorization", r"^Nostr .+"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;

    let env_file = write_env(GOOD_ENV);
    let mut cmd = Command::new(cli_bin());
    cmd.args([
        "bootstrap-power-user",
        "--env",
        env_file.path().to_str().unwrap(),
        "--pubkey",
        TEST_PUBKEY,
        "--pod-url",
        server.uri().as_str(),
        "--force",
    ])
    .env("SERVER_NOSTR_PRIVKEY", TEST_SERVER_SK)
    .env_remove("POWER_USER_NSEC")
    .stdin(Stdio::null());

    let out = cmd.output().expect("run cli");
    assert!(
        out.status.success(),
        "expected exit 0; stderr={}, stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("✓ Wrote"), "stdout={stdout}");
    // Token must never appear in any output stream.
    assert!(
        !stdout.contains("ghp_abcdefghijklmnop1234"),
        "stdout leaked token: {stdout}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("ghp_abcdefghijklmnop1234"),
        "stderr leaked token: {stderr}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn put_non_2xx_causes_error_exit() {
    let server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/private/config/github"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;

    let env_file = write_env(GOOD_ENV);
    let out = Command::new(cli_bin())
        .args([
            "bootstrap-power-user",
            "--env",
            env_file.path().to_str().unwrap(),
            "--pubkey",
            TEST_PUBKEY,
            "--pod-url",
            server.uri().as_str(),
            "--force",
        ])
        .env("SERVER_NOSTR_PRIVKEY", TEST_SERVER_SK)
        .env_remove("POWER_USER_NSEC")
        .stdin(Stdio::null())
        .output()
        .expect("run cli");
    assert!(!out.status.success(), "expected non-zero exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Pod returned HTTP 403"),
        "stderr should mention status, got: {stderr}"
    );
    assert!(
        !stderr.contains("ghp_abcdefghijklmnop1234"),
        "error path leaked token: {stderr}"
    );
}

#[test]
fn dry_run_makes_no_http_request() {
    // We spin up a mock server but attach NO mocks; if the CLI hit it,
    // wiremock would return 404 and the CLI would exit non-zero.  But since
    // dry-run short-circuits, we don't even pass the URL to a live server —
    // we pass a deliberately unreachable host and assert the command still
    // exits 0.
    let env_file = write_env(GOOD_ENV);
    let out = Command::new(cli_bin())
        .args([
            "bootstrap-power-user",
            "--env",
            env_file.path().to_str().unwrap(),
            "--pubkey",
            TEST_PUBKEY,
            "--pod-url",
            "http://127.0.0.1:1", // unreachable port
            "--dry-run",
        ])
        .output()
        .expect("run cli");
    assert!(
        out.status.success(),
        "dry-run must not attempt HTTP (stderr={})",
        String::from_utf8_lossy(&out.stderr)
    );
}

// ---------------------------------------------------------------------------
// wiremock helper
// ---------------------------------------------------------------------------

/// Matcher for a header whose value matches a regex pattern.
fn header_matches(name: &'static str, pattern: &str) -> HeaderRegexMatcher {
    HeaderRegexMatcher {
        name,
        re: regex::Regex::new(pattern).expect("compile regex"),
    }
}

struct HeaderRegexMatcher {
    name: &'static str,
    re: regex::Regex,
}

impl wiremock::Match for HeaderRegexMatcher {
    fn matches(&self, request: &wiremock::Request) -> bool {
        request
            .headers
            .get(self.name)
            .map(|v| self.re.is_match(v.to_str().unwrap_or("")))
            .unwrap_or(false)
    }
}
