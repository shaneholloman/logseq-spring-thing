//! `validate-md` — thin CLI that wraps the JSON-LD validator.
//!
//! Usage:
//!
//! ```text
//! validate-md <path>...
//! ```
//!
//! Each argument is a markdown file path. The binary runs the
//! canonical validator against every file in turn and prints a
//! colourised summary. Exit code is `0` when no `Error`-severity
//! issues were found, `1` otherwise — this is the contract the
//! `pre-commit-validate.sh` hook depends on.

use std::path::PathBuf;
use std::process::ExitCode;

use visionclaw_server::services::jsonld_validator::{Severity, Validator};

const RESET: &str = "\x1b[0m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const GREEN: &str = "\x1b[32m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: validate-md <path>...");
        return ExitCode::from(2);
    }
    let use_colour = should_use_colour();
    let validator = match Validator::new() {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "{}error{}: cannot initialise validator: {}",
                if use_colour { RED } else { "" },
                if use_colour { RESET } else { "" },
                e
            );
            return ExitCode::from(2);
        }
    };

    let mut total_files = 0usize;
    let mut total_errors = 0usize;
    let mut total_warnings = 0usize;
    let mut failed_files: Vec<PathBuf> = Vec::new();

    for arg in args {
        let path = PathBuf::from(&arg);
        total_files += 1;
        let issues = validator.validate_markdown_file(&path);
        if issues.is_empty() {
            if use_colour {
                println!("{}{} {}OK{}", GREEN, path.display(), RESET, DIM);
            } else {
                println!("{} OK", path.display());
            }
            continue;
        }
        let mut file_errors = 0usize;
        for issue in &issues {
            match issue.severity {
                Severity::Error => {
                    total_errors += 1;
                    file_errors += 1;
                    print_issue(&path, issue, use_colour);
                }
                Severity::Warning => {
                    total_warnings += 1;
                    print_issue(&path, issue, use_colour);
                }
            }
        }
        if file_errors > 0 {
            failed_files.push(path);
        }
    }

    println!();
    print_summary(
        total_files,
        total_errors,
        total_warnings,
        &failed_files,
        use_colour,
    );

    if total_errors > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::from(0)
    }
}

fn should_use_colour() -> bool {
    // Honour NO_COLOR (https://no-color.org/) and avoid colour when
    // stdout is not a tty (best-effort heuristic via env var only;
    // adding a dep on `is-terminal` would be overkill).
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    !matches!(std::env::var("TERM").as_deref(), Ok("dumb"))
}

fn print_issue(
    path: &std::path::Path,
    issue: &visionclaw_server::services::jsonld_validator::ValidationIssue,
    use_colour: bool,
) {
    let (sev_colour, sev_label) = match issue.severity {
        Severity::Error => (RED, "error"),
        Severity::Warning => (YELLOW, "warning"),
    };
    if use_colour {
        println!(
            "{bold}{path}{reset}:{loc} {col}{sev}{reset}[{code}]: {msg}",
            bold = BOLD,
            path = path.display(),
            reset = RESET,
            loc = loc(&issue.source),
            col = sev_colour,
            sev = sev_label,
            code = issue.category.code(),
            msg = issue.message,
        );
        if let Some(fix) = &issue.suggested_fix {
            println!("  {}fix:{} {}", DIM, RESET, fix);
        }
    } else {
        println!(
            "{}:{} {}[{}]: {}",
            path.display(),
            loc(&issue.source),
            sev_label,
            issue.category.code(),
            issue.message
        );
        if let Some(fix) = &issue.suggested_fix {
            println!("  fix: {}", fix);
        }
    }
}

fn loc(source: &visionclaw_server::services::jsonld_validator::SourceRef) -> String {
    match (source.line, source.column, &source.block_label) {
        (Some(l), Some(c), Some(b)) => format!("{}:{} ({})", l, c, b),
        (Some(l), Some(c), None) => format!("{}:{}", l, c),
        (Some(l), None, Some(b)) => format!("{} ({})", l, b),
        (Some(l), None, None) => format!("{}", l),
        (None, _, Some(b)) => format!("- ({})", b),
        _ => "-".to_string(),
    }
}

fn print_summary(
    total_files: usize,
    errors: usize,
    warnings: usize,
    failed: &[PathBuf],
    use_colour: bool,
) {
    if errors == 0 && warnings == 0 {
        if use_colour {
            println!(
                "{}validation passed{}: {} files, 0 errors, 0 warnings",
                GREEN, RESET, total_files
            );
        } else {
            println!(
                "validation passed: {} files, 0 errors, 0 warnings",
                total_files
            );
        }
        return;
    }
    if use_colour {
        println!(
            "{}validation summary{}: {} files, {}{} errors{}, {}{} warnings{}",
            BOLD,
            RESET,
            total_files,
            if errors > 0 { RED } else { GREEN },
            errors,
            RESET,
            if warnings > 0 { YELLOW } else { GREEN },
            warnings,
            RESET
        );
    } else {
        println!(
            "validation summary: {} files, {} errors, {} warnings",
            total_files, errors, warnings
        );
    }
    if !failed.is_empty() {
        println!("failed files:");
        for f in failed {
            println!("  - {}", f.display());
        }
    }
}
