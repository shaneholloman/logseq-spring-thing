#!/bin/bash
# agentbox-post-start.sh — Run after agentbox container starts
# Validates health and copies any workspace data from legacy MAD if present
set -euo pipefail

CYAN='\033[0;36m'
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${CYAN}=== Agentbox Post-Start Setup ===${NC}"

# 1. Wait for agentbox container to be healthy
echo -e "${CYAN}Waiting for agentbox health...${NC}"
deadline=$(($(date +%s) + 120))
while [ $(date +%s) -lt $deadline ]; do
    if docker exec agentbox curl -sf http://localhost:9190/health >/dev/null 2>&1; then
        echo -e "${GREEN}Agentbox is healthy${NC}"
        break
    fi
    sleep 3
done

# 2. Copy agent templates from legacy MAD (if still running)
echo -e "${CYAN}Copying agent templates (if legacy MAD available)...${NC}"
docker cp agentic-workstation:/home/devuser/agents/. /tmp/agents-transfer/ 2>/dev/null || true
if [ -d /tmp/agents-transfer ]; then
    docker cp /tmp/agents-transfer/. agentbox:/opt/agentbox/agents/ 2>/dev/null || \
    docker cp /tmp/agents-transfer/. agentbox:/home/devuser/agents/ 2>/dev/null || true
    echo -e "${GREEN}Agent templates copied ($(ls /tmp/agents-transfer/*.md 2>/dev/null | wc -l) files)${NC}"
    rm -rf /tmp/agents-transfer
fi

# 3. Copy .claude settings from legacy MAD (if still running)
echo -e "${CYAN}Syncing Claude settings (if legacy MAD available)...${NC}"
docker cp agentic-workstation:/home/devuser/.claude/settings.json /tmp/claude-settings.json 2>/dev/null || true
if [ -f /tmp/claude-settings.json ]; then
    docker cp /tmp/claude-settings.json agentbox:/home/devuser/.claude/settings.json 2>/dev/null || true
    echo -e "${GREEN}Claude settings synced${NC}"
    rm -f /tmp/claude-settings.json
fi

# 4. Copy workspace data that isn't externally mounted
echo -e "${CYAN}Copying workspace state...${NC}"
for dir in .claude-flow .hive-mind .swarm .mcp.json; do
    docker cp "agentic-workstation:/home/devuser/workspace/${dir}" "/tmp/ws-transfer-${dir}" 2>/dev/null || true
    if [ -e "/tmp/ws-transfer-${dir}" ]; then
        docker cp "/tmp/ws-transfer-${dir}" "agentbox:/home/devuser/workspace/${dir}" 2>/dev/null || true
        rm -rf "/tmp/ws-transfer-${dir}"
    fi
done

# 5. Verify services
echo -e "${CYAN}Checking service status...${NC}"
docker exec agentbox supervisorctl status 2>/dev/null || echo -e "${RED}supervisorctl not available${NC}"

echo ""
echo -e "${GREEN}=== Agentbox post-start complete ===${NC}"
echo -e "  Management API: http://localhost:9190"
echo -e "  Code Server:    http://localhost:8180"
echo -e "  VNC Desktop:    vnc://localhost:5902"
echo -e "  SSH:            ssh -p 2223 devuser@localhost"
echo -e "  Metrics:        http://localhost:9191/metrics"
echo -e "  Solid Pod:      http://localhost:8484"
echo -e "  Shell:          docker exec -it agentbox bash"
