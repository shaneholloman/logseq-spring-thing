#!/usr/bin/env bash
# pre-commit-validate.sh — Canonical JSON-LD schema gate.
#
# Runs the `validate-md` binary against every staged *.md file. Blocks
# the commit if any file has Error-severity issues. Authors can bypass
# with `git commit --no-verify`, but the pre-ingest validator inside
# the webxr pipeline will catch the same violations later — escaping
# the pre-commit hook is not the same as escaping validation.
#
# Install (from the project root):
#
#   ln -sf ../../scripts/pre-commit-validate.sh .git/hooks/pre-commit
#   chmod +x .git/hooks/pre-commit
#
# Or invoke directly:
#
#   ./scripts/pre-commit-validate.sh
#
# The hook compiles the binary on the first run; subsequent runs are
# fast because cargo caches.

set -euo pipefail

# Colour control. Match the binary's NO_COLOR semantics.
if [[ -t 1 && -z "${NO_COLOR:-}" ]]; then
    C_RED=$'\e[31m'
    C_GREEN=$'\e[32m'
    C_YELLOW=$'\e[33m'
    C_BOLD=$'\e[1m'
    C_DIM=$'\e[2m'
    C_RESET=$'\e[0m'
else
    C_RED=""
    C_GREEN=""
    C_YELLOW=""
    C_BOLD=""
    C_DIM=""
    C_RESET=""
fi

repo_root() {
    git rev-parse --show-toplevel
}

ROOT="$(repo_root)"
cd "$ROOT"

# Collect staged markdown files. Use --diff-filter=ACM so we ignore
# deletes — there's nothing to validate when a file is removed.
mapfile -t STAGED < <(git diff --cached --name-only --diff-filter=ACM -- '*.md' || true)

if [[ ${#STAGED[@]} -eq 0 ]]; then
    echo "${C_DIM}pre-commit-validate: no staged markdown files; nothing to check${C_RESET}"
    exit 0
fi

echo "${C_BOLD}pre-commit-validate${C_RESET}: checking ${#STAGED[@]} staged markdown file(s)"

# Build the binary if missing. `--quiet` suppresses cargo's progress
# noise; `--bin validate-md` builds JUST that binary so we don't pay
# the full-workspace compile cost on every commit.
BINARY="$ROOT/target/debug/validate-md"
if [[ ! -x "$BINARY" ]]; then
    echo "${C_DIM}building validate-md...${C_RESET}"
    if ! cargo build --quiet --bin validate-md 2>&1; then
        echo "${C_RED}error${C_RESET}: failed to build validate-md"
        echo "  fix: run \`cargo build --bin validate-md\` and address the build error"
        exit 2
    fi
fi

# Run validator. Pass through staged paths; the binary handles
# missing/unreadable files itself.
set +e
"$BINARY" "${STAGED[@]}"
status=$?
set -e

if [[ $status -ne 0 ]]; then
    echo
    echo "${C_RED}${C_BOLD}commit blocked${C_RESET}: schema violations in staged files."
    echo "  - To inspect: ${C_BOLD}./target/debug/validate-md <path>${C_RESET}"
    echo "  - To override (NOT recommended): ${C_BOLD}git commit --no-verify${C_RESET}"
    echo "  - Pre-ingest validator will reject the same violations downstream."
    exit "$status"
fi

echo "${C_GREEN}${C_BOLD}pre-commit-validate: all files pass${C_RESET}"
exit 0
