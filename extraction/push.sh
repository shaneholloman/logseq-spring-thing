#!/usr/bin/env bash
# push.sh — create dreamlab-ai/solid-pod-rs on GitHub, restore history
# from the bundle, push the extracted source, and tag v0.3.0-alpha.1.
#
# Usage:
#     ./push.sh                 # creates new public repo + pushes
#     ./push.sh --no-create     # assumes repo already exists
#
# If `gh` is not authenticated, the script prints the manual commands
# you can run instead and exits 0 without touching any remote.

set -euo pipefail

REPO_OWNER="dreamlab-ai"
REPO_NAME="solid-pod-rs"
REPO_SLUG="${REPO_OWNER}/${REPO_NAME}"
REPO_DESC='Rust-native Solid Pod server (LDP + WAC + NIP-98 + OIDC + Notifications)'
TAG="v0.3.0-alpha.1"
COMMIT_MSG='feat: initial release as standalone repository'

# Resolve paths relative to this script.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC_DIR="${SCRIPT_DIR}/solid-pod-rs"
BUNDLE="${SCRIPT_DIR}/solid-pod-rs-history.bundle"

CREATE_REPO=1
if [[ "${1:-}" == "--no-create" ]]; then
    CREATE_REPO=0
fi

log()  { printf '\033[1;34m[push.sh]\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31m[push.sh]\033[0m %s\n' "$*" >&2; exit 1; }

[[ -d "${SRC_DIR}" ]]   || fail "missing ${SRC_DIR}"
[[ -f "${BUNDLE}"   ]]  || fail "missing ${BUNDLE}"

# --- Check gh auth -------------------------------------------------------

if ! command -v gh >/dev/null 2>&1; then
    log "gh CLI not found. Install from https://cli.github.com"
    log "Printing manual commands below and exiting."
    GH_OK=0
elif ! gh auth status >/dev/null 2>&1; then
    log "gh not authenticated. Run 'gh auth login' first."
    log "Printing manual commands below and exiting."
    GH_OK=0
else
    GH_OK=1
fi

print_manual() {
    cat <<EOF

=== Manual fallback =====================================================

1. Create the repo in the browser:
       https://github.com/organizations/${REPO_OWNER}/repositories/new
   Name: ${REPO_NAME}
   Visibility: Public
   Description: ${REPO_DESC}
   Do NOT initialise with README/LICENSE/gitignore.

2. Restore the extraction bundle into the source tree:
       cd "${SRC_DIR}"
       git init --initial-branch=main
       git fetch "${BUNDLE}" solid-pod-rs-history:solid-pod-rs-history
       git merge --allow-unrelated-histories --no-edit solid-pod-rs-history || true
       git add -A
       git commit -m "${COMMIT_MSG}"
       git remote add origin git@github.com:${REPO_SLUG}.git
       git push -u origin main
       git tag -a ${TAG} -m "${TAG}"
       git push origin ${TAG}

3. Optional: enable branch protection, Dependabot, and Actions.

=========================================================================
EOF
}

if [[ "${GH_OK}" -eq 0 ]]; then
    print_manual
    exit 0
fi

# --- Create repo ---------------------------------------------------------

if [[ "${CREATE_REPO}" -eq 1 ]]; then
    log "Creating public repo ${REPO_SLUG} via gh..."
    if gh repo view "${REPO_SLUG}" >/dev/null 2>&1; then
        log "Repo already exists; skipping create."
    else
        gh repo create "${REPO_SLUG}" \
            --public \
            --description "${REPO_DESC}" \
            --homepage "https://github.com/${REPO_SLUG}" \
            --disable-wiki
    fi
else
    log "Skipping repo creation (--no-create)."
fi

# --- Init git in extracted dir ------------------------------------------

cd "${SRC_DIR}"

if [[ -d ".git" ]]; then
    log "Existing .git in ${SRC_DIR}; using it."
else
    log "git init in ${SRC_DIR}"
    git init --initial-branch=main
fi

# --- Restore history from bundle, then merge working tree on top --------

log "Fetching extraction history from bundle..."
git fetch "${BUNDLE}" 'solid-pod-rs-history:solid-pod-rs-history'

# If this is a fresh init with no commits, start from the history branch.
if ! git rev-parse --verify HEAD >/dev/null 2>&1; then
    log "Starting main from extracted history."
    git checkout -B main solid-pod-rs-history
fi

log "Staging current working tree (verbatim extraction snapshot)..."
git add -A

if git diff --cached --quiet; then
    log "No changes to commit; working tree matches extracted history."
else
    git -c user.name='solid-pod-rs bot' \
        -c user.email='maintainers@dreamlab.ai' \
        commit -m "${COMMIT_MSG}" \
        -m 'Extracted from github.com/DreamLab-AI/VisionClaw:crates/solid-pod-rs/ on 2026-04-20.' \
        -m 'See NOTICE for the full provenance chain (VisionClaw -> community-forum-rs pod-worker -> CSS reference).'
fi

# --- Wire remote + push --------------------------------------------------

if git remote get-url origin >/dev/null 2>&1; then
    log "Updating existing origin remote."
    git remote set-url origin "git@github.com:${REPO_SLUG}.git"
else
    log "Adding origin remote."
    git remote add origin "git@github.com:${REPO_SLUG}.git"
fi

log "Pushing main to origin..."
git push -u origin main

# --- Tag -----------------------------------------------------------------

if git rev-parse "${TAG}" >/dev/null 2>&1; then
    log "Tag ${TAG} already exists locally; skipping creation."
else
    log "Tagging ${TAG}."
    git tag -a "${TAG}" -m "${TAG} — initial release as standalone repo"
fi

log "Pushing tag ${TAG}..."
git push origin "${TAG}" || log "Tag push failed; re-run manually if needed."

log "Done."
log "Repo:   https://github.com/${REPO_SLUG}"
log "Tag:    ${TAG}"
log "Bundle: ${BUNDLE} (contains the pre-extraction crate history)"
