#!/usr/bin/env bash
# Tag-and-bag accumulated one-shot scripts into scripts/_archive/.
# Mission-critical scripts (launch, entrypoints, build wrappers, supervisord
# refs, pre-commit hooks) stay at scripts/ root.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)/scripts"

mkdir -p _archive/{rename,migrate,refactor,fix,audit,test-filters,one-shot-tests,legacy-memory,cleanup}

# ----- Rename scripts (one-shot, this sprint) -----
git mv rename-webxr-to-visionflow-server.sh _archive/rename/ 2>/dev/null || true
git mv rename-visionflow-to-visionclaw.sh   _archive/rename/ 2>/dev/null || true
git mv rename-visionflow-bulk.sh             _archive/rename/ 2>/dev/null || true

# ----- Migration scripts (historical, completed migrations) -----
for f in \
  bulk_migrate_unwraps.sh \
  categorize_unwraps.sh \
  migrate_complex_errors.sh \
  migrate_complex_final.py \
  migrate_format_errors.py \
  migrate_json_unwraps.sh \
  migrate_legacy_configs.rs \
  migrate_ontology_database.sql \
  migrate_responses.py \
  migrate_responses_final.py \
  migrate_responses_v2.py \
  migrate_unwraps_batch.py \
  migrate_with_extended_macros.py \
  migrate_all_remaining.py \
  run_migration.rs \
  run_migration.sh \
  validate_migration.sh \
  migrate-env.js \
  import-sqlite-to-pg.js \
; do
  [ -f "$f" ] && git mv "$f" _archive/migrate/ 2>/dev/null || true
done

# ----- Refactor scripts (one-shot codebase-wide rewrites) -----
for f in \
  refactor_http_responses.sh \
  refactor_responses.py \
  refactor_responses_phase2.py \
  refactor_responses_phase3.py \
  replace_time_operations.sh \
  remove-comments.sh \
  remove-rust-comments.sh \
; do
  [ -f "$f" ] && git mv "$f" _archive/refactor/ 2>/dev/null || true
done

# ----- Fix scripts (one-shot patch passes) -----
for f in \
  fix-mcp-patches.sh \
  fix-mermaid-diagrams.py \
  fix-ontology-schema.sql \
  fix-ontology-schema-v2.sql \
  fix-type-a-assets.py \
  fix-uk-spellings.py \
  fix-uk-spellings.sh \
  fix_kokoro_network.sh \
  manual-fix-agent-list.sh \
; do
  [ -f "$f" ] && git mv "$f" _archive/fix/ 2>/dev/null || true
done

# ----- Audit scripts -----
for f in \
  analyze_doc_links.py \
  analyze_production_unwraps.py \
  gpt54-audit-split.py \
  gpt54-audit.py \
  monitor-audit-completion.sh \
; do
  [ -f "$f" ] && git mv "$f" _archive/audit/ 2>/dev/null || true
done

# ----- Filter-debug test one-shots (17 files chasing one bug) -----
for f in \
  test-analytics-filter.ts \
  test-filter-debug.ts \
  test-filter-direct.ts \
  test-filter-e2e.ts \
  test-filter-e2e-v2.ts \
  test-filter-final.ts \
  test-filter-final2.ts \
  test-filter-functionality.ts \
  test-filter-keyboard.ts \
  test-filter-playwright.ts \
  test-filter-release.ts \
  test-filter-simple.ts \
  test-filter-store.ts \
  test-filter-sync.ts \
  test-filter-threshold.ts \
  test-filter-verify.ts \
  test-filter-via-store.ts \
  test-filter-websocket.ts \
  test-filtering.ts \
  test-fresh-client-filter.ts \
  test-node-filter.ts \
  test-quality-filter-verification.ts \
  test-quality-tab-filter.ts \
; do
  [ -f "$f" ] && git mv "$f" _archive/test-filters/ 2>/dev/null || true
done

# ----- Other one-shot tests -----
for f in \
  test-control-center.ts \
  test-ws-node-count.ts \
  test-mcp-patch.sh \
  test-physics-update.sh \
  test-settings-cache.sh \
  test_compile.sh \
  test_hot_reload.sh \
  test_kokoro_tts.sh \
  test_logging_integration.py \
  test_mcp_connection.rs \
  test_mcp_direct.sh \
  test_mcp_server.py \
  test_validation.sh \
  test_voice_pipeline.sh \
  test_whisper_stt.sh \
  voice_pipeline_test.sh \
  final-test.sh \
  quick_test_validation.sh \
; do
  [ -f "$f" ] && git mv "$f" _archive/one-shot-tests/ 2>/dev/null || true
done

# ----- Legacy memory migration -----
for f in \
  migrate-legacy-memory.py \
  query-legacy-memory.py \
  memory-flash-bridge.mjs \
  setup-memory-flash-trigger.sql \
; do
  [ -f "$f" ] && git mv "$f" _archive/legacy-memory/ 2>/dev/null || true
done

# ----- Data cleanup one-shots -----
for f in \
  clean_all_graph_data.sql \
  clean_github_data.sql \
  populate_test_data.py \
  load_test_settings.sh \
  trigger_physics_update.sh \
  trigger_sync.sh \
  update_physics_direct.sh \
  update_physics_settings.sh \
; do
  [ -f "$f" ] && git mv "$f" _archive/cleanup/ 2>/dev/null || true
done

# ----- Stray log files (NOT scripts) -----
[ -f gpu-test-execution.log ] && git mv gpu-test-execution.log _archive/ 2>/dev/null || true

# ----- Old scripts/tests subdir (probably one-shots) -----
# Leave scripts/tests/ in place unless it's obviously stale — preserve.

echo
echo "Remaining in scripts/ root:"
ls scripts/ 2>&1 | head -40
echo "..."
echo
echo "Archived to scripts/_archive/:"
find scripts/_archive -type f 2>/dev/null | wc -l
echo "files"
