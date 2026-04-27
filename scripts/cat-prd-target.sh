#!/bin/bash
# Concatenates all files relevant to:
#   Group 1: Multi-agent Docker container build
#   Group 2: solid-pod-rs integration
# into agentbox-prd-target.txt at the project root.
set -euo pipefail

OUT="$(cd "$(dirname "$0")/.." && pwd)/agentbox-prd-target.txt"
cd "$(dirname "$0")/.."

> "$OUT"

emit() {
  local f="$1"
  if [ -f "$f" ]; then
    echo "" >> "$OUT"
    echo "================================================================================" >> "$OUT"
    echo "FILE: $f" >> "$OUT"
    echo "================================================================================" >> "$OUT"
    cat "$f" >> "$OUT"
    echo "" >> "$OUT"
  else
    echo "WARNING: $f not found, skipping" >&2
  fi
}

echo "Writing to $OUT ..."

# ============================================================================
# GROUP 1: Multi-Agent Docker Build Files
# ============================================================================

echo "" >> "$OUT"
echo "###############################################################################" >> "$OUT"
echo "# GROUP 1: MULTI-AGENT DOCKER BUILD FILES" >> "$OUT"
echo "###############################################################################" >> "$OUT"

# --- 1. Build Orchestration & Entry Points ---
emit scripts/launch.sh
emit multi-agent-docker/build-unified.sh
emit multi-agent-docker/REBUILD.sh

# --- 2. Dockerfiles ---
emit multi-agent-docker/Dockerfile.unified
emit multi-agent-docker/ruvector-postgres/Dockerfile
emit multi-agent-docker/claude-zai/Dockerfile
emit multi-agent-docker/claude-zai/Dockerfile.cachyos
emit multi-agent-docker/comfyui/Dockerfile
emit multi-agent-docker/comfyui/Dockerfile.cachyos

# --- 3. Docker Compose Files ---
emit multi-agent-docker/docker-compose.unified.yml
emit multi-agent-docker/docker-compose.visionflow-cachyos.yml
emit multi-agent-docker/comfyui/docker-compose.comfyui.yml
emit docker-compose.unified.yml
emit docker-compose.dev.yml
emit docker-compose.production.yml
emit docker-compose.yml
emit docker-compose.voice.yml

# --- 4. Environment & Configuration ---
emit multi-agent-docker/.env.example
emit multi-agent-docker/.gitignore
emit multi-agent-docker/config/cuda-compatibility.yml
emit multi-agent-docker/config/gemini-flow.config.ts

# --- 5. Entrypoint & Runtime Init ---
emit multi-agent-docker/unified-config/entrypoint-unified.sh
emit multi-agent-docker/unified-config/supervisord.unified.conf
emit multi-agent-docker/unified-config/autostart-terminals.sh
emit multi-agent-docker/unified-config/tmux-autostart.sh
emit multi-agent-docker/unified-config/disable-screensaver.sh
emit multi-agent-docker/unified-config/statusline.sh
emit multi-agent-docker/unified-config/10-headless.conf
emit multi-agent-docker/unified-config/hyprland.conf
emit multi-agent-docker/unified-config/kitty.conf

# --- 6. Terminal Init Scripts ---
emit multi-agent-docker/unified-config/terminal-init/init-claude-agent.sh
emit multi-agent-docker/unified-config/terminal-init/init-claude-main.sh
emit multi-agent-docker/unified-config/terminal-init/init-deepseek.sh
emit multi-agent-docker/unified-config/terminal-init/init-development.sh
emit multi-agent-docker/unified-config/terminal-init/init-docker.sh
emit multi-agent-docker/unified-config/terminal-init/init-gemini.sh
emit multi-agent-docker/unified-config/terminal-init/init-git.sh
emit multi-agent-docker/unified-config/terminal-init/init-openai.sh
emit multi-agent-docker/unified-config/terminal-init/init-services.sh
emit multi-agent-docker/unified-config/terminal-init/init-zai.sh

# --- 7. Runtime Scripts ---
emit multi-agent-docker/unified-config/turbo-flow-aliases.sh
emit multi-agent-docker/unified-config/claude-flow-config.json
emit multi-agent-docker/unified-config/init-ruvector.sql
emit multi-agent-docker/unified-config/scripts/as-gemini.sh
emit multi-agent-docker/unified-config/scripts/as-openai.sh
emit multi-agent-docker/unified-config/scripts/as-zai.sh
emit multi-agent-docker/unified-config/scripts/deepseek-chat.js
emit multi-agent-docker/unified-config/scripts/generate-mcp-settings.sh
emit multi-agent-docker/unified-config/scripts/local-llm-proxy.mjs
emit multi-agent-docker/unified-config/scripts/project-env.sh
emit multi-agent-docker/unified-config/scripts/project-identify.sh
emit multi-agent-docker/unified-config/scripts/services-status.sh
emit multi-agent-docker/unified-config/scripts/setup-deepseek-user.sh
emit multi-agent-docker/unified-config/scripts/skill-list.sh
emit multi-agent-docker/unified-config/scripts/ssh-setup.sh
emit multi-agent-docker/unified-config/scripts/verify-management-api.sh
emit multi-agent-docker/unified-config/scripts/vnc-info.sh

# --- 8. RuVector Memory Bridge ---
emit multi-agent-docker/unified-config/ruflo-memory-bridge-pg.js
emit multi-agent-docker/unified-config/ruflo-memory-initializer-pg.js
emit multi-agent-docker/unified-config/ruflo-memory-tools-pg.js
emit multi-agent-docker/unified-config/ruflo-pg-memory-bridge.js

# --- 9. MCP Infrastructure ---
emit multi-agent-docker/mcp-infrastructure/package.json
emit multi-agent-docker/mcp-infrastructure/package-lock.json
emit multi-agent-docker/mcp-infrastructure/mcp.json
emit multi-agent-docker/mcp-infrastructure/mcp-full-registry.json
emit multi-agent-docker/mcp-infrastructure/servers/mcp-server.js
emit multi-agent-docker/mcp-infrastructure/servers/mcp-gateway.js
emit multi-agent-docker/mcp-infrastructure/servers/mcp-tcp-server.js
emit multi-agent-docker/mcp-infrastructure/servers/mcp-ws-relay.js
emit multi-agent-docker/mcp-infrastructure/servers/implementations/ragflow-tools.js
emit multi-agent-docker/mcp-infrastructure/auth/auth-middleware.js
emit multi-agent-docker/mcp-infrastructure/auth/secure-client-example.js
emit multi-agent-docker/mcp-infrastructure/config/tools-manifest.json
emit multi-agent-docker/mcp-infrastructure/config/.tools-manifest.lock
emit multi-agent-docker/mcp-infrastructure/config/topics.json
emit multi-agent-docker/mcp-infrastructure/logging/winston-config.js
emit multi-agent-docker/mcp-infrastructure/monitoring/health-check.js
emit multi-agent-docker/mcp-infrastructure/monitoring/health-check.sh
emit multi-agent-docker/mcp-infrastructure/monitoring/check-setup-status.sh
emit multi-agent-docker/mcp-infrastructure/scripts/automated-setup.sh
emit multi-agent-docker/mcp-infrastructure/scripts/claude-flow-tcp-proxy.js
emit multi-agent-docker/mcp-infrastructure/scripts/init-claude-flow-agents.sh

# --- 10. Management API ---
emit multi-agent-docker/management-api/package.json
emit multi-agent-docker/management-api/server.js
emit multi-agent-docker/management-api/middleware/auth.js
emit multi-agent-docker/management-api/hooks/agent-action-hooks.js
emit multi-agent-docker/management-api/routes/agent-events.js
emit multi-agent-docker/management-api/routes/briefs.js
emit multi-agent-docker/management-api/routes/comfyui.js
emit multi-agent-docker/management-api/routes/status.js
emit multi-agent-docker/management-api/routes/tasks.js
emit multi-agent-docker/management-api/services/beads-service.js
emit multi-agent-docker/management-api/services/briefing-service.js
emit multi-agent-docker/management-api/utils/agent-event-bridge.js
emit multi-agent-docker/management-api/utils/agent-event-publisher.js
emit multi-agent-docker/management-api/utils/comfyui-manager.js
emit multi-agent-docker/management-api/utils/logger.js
emit multi-agent-docker/management-api/utils/metrics.js
emit multi-agent-docker/management-api/utils/metrics-comfyui-extension.js
emit multi-agent-docker/management-api/utils/process-manager.js
emit multi-agent-docker/management-api/utils/system-monitor.js

# --- 11. Claude ZAI Service ---
emit multi-agent-docker/claude-zai/Dockerfile
emit multi-agent-docker/claude-zai/Dockerfile.cachyos
emit multi-agent-docker/claude-zai/claude-config.json
emit multi-agent-docker/claude-zai/wrapper/package.json
emit multi-agent-docker/claude-zai/wrapper/server.js

# --- 12. ComfyUI Service ---
emit multi-agent-docker/comfyui/Dockerfile
emit multi-agent-docker/comfyui/Dockerfile.cachyos
emit multi-agent-docker/comfyui/docker-compose.comfyui.yml
emit multi-agent-docker/comfyui/runner-scripts/entrypoint.sh
emit multi-agent-docker/comfyui/runner-scripts/entrypoint-with-sam3d-fix.sh
emit multi-agent-docker/comfyui/runner-scripts/fix-sam3d-on-startup.sh
emit multi-agent-docker/comfyui/scripts/fix-sam3d-cuda.sh
emit multi-agent-docker/comfyui/scripts/patch-sam3d-bridge.py
emit multi-agent-docker/comfyui/workflows/flux2-phase1-generate.json
emit multi-agent-docker/comfyui/workflows/flux2-phase2-sam3d-rmbg.json
emit multi-agent-docker/comfyui/workflows/flux2-phase2-sam3d.json
emit multi-agent-docker/comfyui/workflows/flux2-sam3d-retro-skyscraper.json

# --- 13. AISP Integration ---
emit multi-agent-docker/aisp-integration/package.json
emit multi-agent-docker/aisp-integration/index.js
emit multi-agent-docker/aisp-integration/cli.js
emit multi-agent-docker/aisp-integration/benchmark.js
emit multi-agent-docker/aisp-integration/init-aisp.sh

# --- 14. HTTPS Bridge ---
emit multi-agent-docker/https-bridge/package.json
emit multi-agent-docker/https-bridge/https-proxy.js

# --- 15. Skills (SKILL.md files) ---
emit multi-agent-docker/skills/mcp.json
emit multi-agent-docker/skills/SKILL-DIRECTORY.md
for skill_dir in multi-agent-docker/skills/*/; do
  if [ -f "${skill_dir}SKILL.md" ]; then
    emit "${skill_dir}SKILL.md"
  fi
done

# --- 16. Schemas ---
emit multi-agent-docker/schemas/agent-memory.jsonld

# --- 17. DevContainer ---
emit multi-agent-docker/.devcontainer/devcontainer.json

# --- 18. Package Manifests ---
emit multi-agent-docker/package.json

# --- 19. Agent Behavior Rules ---
emit multi-agent-docker/CLAUDE.md
emit multi-agent-docker/CLAUDE.workspace.md

# --- 20. Multi-agent-docker helper scripts ---
emit multi-agent-docker/scripts/claude-md-dedup.sh
emit multi-agent-docker/scripts/validate-docker-manager.sh

# --- 21. Documentation ---
emit multi-agent-docker/README.md
emit multi-agent-docker/QUICKSTART.md
emit multi-agent-docker/DOCKER-BUILD-NOTES.md
emit multi-agent-docker/SSH-SETUP.md
emit multi-agent-docker/SSH-SETUP-CHANGES.md
emit multi-agent-docker/CHANGELOG-QGIS.md
emit multi-agent-docker/docs/CACHYOS-ARCHITECTURE.md
emit multi-agent-docker/docs/LOCAL-LLM-PROXY.md
emit multi-agent-docker/docs/RUVECTOR-MEMORY.md
emit multi-agent-docker/docs/AISP-INTEGRATION.md
emit multi-agent-docker/docs/SKILLS-PORTFOLIO.md
emit multi-agent-docker/docs/adr/ADR-001-v4-upgrade-strategy.md

# ============================================================================
# GROUP 2: solid-pod-rs Integration Files
# ============================================================================

echo "" >> "$OUT"
echo "###############################################################################" >> "$OUT"
echo "# GROUP 2: SOLID-POD-RS INTEGRATION FILES" >> "$OUT"
echo "###############################################################################" >> "$OUT"

# --- Dependency ---
emit Cargo.toml

# --- Rust Source: Direct crate imports ---
emit src/handlers/solid_pod_handler.rs
emit src/handlers/mod.rs
emit src/main.rs

# --- Rust Source: PodClient and consumers ---
emit src/services/pod_client.rs
emit src/services/ingest_saga.rs
emit src/services/metrics.rs
emit src/services/wac_mutator.rs
emit src/services/type_index_discovery.rs
emit src/services/inbox_service.rs
emit src/services/github_sync_service.rs
emit src/services/mod.rs
emit src/actors/dojo_discovery_actor.rs
emit src/services/parsers/knowledge_graph_parser.rs
emit src/handlers/image_gen_handler.rs
emit src/utils/nip98.rs
emit src/sovereign/visibility.rs
emit src/domain/contributor/context_assembly.rs
emit src/config/oidc.rs
emit src/bin/vc_cli.rs

# --- Frontend TypeScript ---
emit client/src/services/SolidPodService.ts
emit client/src/features/solid/hooks/useSolidPod.ts
emit client/src/features/solid/hooks/useSolidResource.ts
emit client/src/features/solid/hooks/useSolidContainer.ts
emit client/src/features/solid/components/PodBrowser.tsx
emit client/src/features/solid/components/PodSettings.tsx
emit client/src/features/solid/components/ResourceEditor.tsx
emit client/src/features/solid/components/SolidTabContent.tsx
emit client/src/features/solid/components/index.ts
emit client/src/features/solid/hooks/index.ts
emit client/src/features/solid/index.ts
emit client/src/features/visualisation/components/IntegratedControlPanel.tsx
emit client/src/__tests__/agent-pod/pod-provisioning.test.ts
emit client/tests/e2e/solid/solid-integration.spec.ts

# --- Config / Deployment ---
emit config/staging/docker-compose.staging.yml
emit config/staging/grafana/dashboards/sovereign-mesh-overview.json

# --- Key ADRs and docs ---
emit docs/adr/ADR-053-solid-pod-rs-crate-extraction.md
emit docs/adr/ADR-056-jss-parity-migration.md
emit docs/ops/jss-native-cutover.md
emit docs/prd/jss-parity-migration.md

# ============================================================================
# Done
# ============================================================================

SIZE=$(wc -c < "$OUT")
LINES=$(wc -l < "$OUT")
echo "Done. $OUT is $SIZE bytes, $LINES lines."
