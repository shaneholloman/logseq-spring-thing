# agentbox — Operational Runbook

| Field | Value |
|-------|-------|
| Substrate | agentbox |
| Repo | github.com/DreamLab-AI/agentbox (submodule in VisionClaw) |
| Runtime | Nix-based container (Docker) |
| Ports | 9190 (mgmt API), 8180 (code-server), 5902 (VNC), 2223 (SSH), 8484 (Solid Pod), 9700 (agent events), 9191 (metrics) |

## Architecture

Sovereign agentic container with pluggable adapter architecture (ADR-005):
- Management API (Express.js, port 9190)
- nostr-rs-relay (embedded, NIP-42 AUTH)
- Solid Pod server (solid-pod-rs, port 8484)
- Process manager (supervisord)
- Agent events WebSocket (port 9700)

## Startup / Shutdown

```bash
# Start (from host, via docker-compose)
docker compose --profile dev up -d agentbox

# Stop
docker compose --profile dev stop agentbox

# Restart management API only
docker exec agentbox supervisorctl restart management-api

# Check all services
docker exec agentbox supervisorctl status
```

## Health Checks

| Endpoint | Port | Expected |
|----------|------|----------|
| GET /health | 9190 | 200 `{"status":"ok","services":{...}}` |
| GET /metrics | 9191 | 200 Prometheus metrics |
| GET /.well-known/solid | 8484 | 200 JSON-LD |

## Common Failure Modes

### Process Manager Crashes
- **Symptom**: Individual services stop responding
- **Cause**: OOM, uncaught exception, or Nix environment issue
- **Fix**: `docker exec agentbox supervisorctl restart <service>`

### Agent Task Spawn Failure
- **Symptom**: POST /api/tasks returns 500
- **Cause**: Process limit reached or disk full
- **Note**: `--dangerously-skip-permissions` is by design (sovereign container)
- **Fix**: Check `docker exec agentbox df -h` and `supervisorctl status`

### Solid Pod Bridge Unavailable
- **Symptom**: 502 on pod endpoints
- **Cause**: solid-pod-rs service not started or port conflict
- **Fix**: `docker exec agentbox supervisorctl restart solid-pod`

### Nostr Relay Connection Refused
- **Symptom**: WebSocket connections to relay fail
- **Cause**: nostr-rs-relay not running or NIP-42 AUTH misconfigured
- **Fix**: Check `docker exec agentbox supervisorctl status nostr-relay`

## Backup / Restore

- **Agent state**: Ephemeral by design. Long-running state in Solid Pod.
- **Solid Pod data**: `docker exec agentbox tar czf /backup/pod-data.tar.gz /data/pods/`
- **Relay events**: nostr-rs-relay SQLite at `/data/relay/nostr.db`. Copy for backup.
- **Configuration**: Git-tracked in `agentbox/` submodule. No runtime config to back up.

## RTO / RPO Targets

| Component | RTO | RPO | Notes |
|-----------|-----|-----|-------|
| Container | < 2 min | N/A | `docker compose up` restarts |
| Management API | < 30 sec | N/A | Stateless, supervisord auto-restart |
| Solid Pod | < 2 min | < 1h | Restore from tar backup |
| Relay | < 2 min | < 24h | Restore SQLite, events re-sync from peers |
| Agent tasks | N/A | N/A | Ephemeral by design |

## Monitoring

- `docker exec agentbox supervisorctl status` — service health
- `curl localhost:9191/metrics` — Prometheus metrics
- `docker logs agentbox --tail 100` — container logs
- Agent events: WebSocket at `ws://localhost:9700/ws/events`

## Security Notes

- `--dangerously-skip-permissions` is accepted by design (PRD-014 S04)
- Container runs as isolated unit; host access via Docker socket only
- NIP-42 AUTH gates relay access
- Management API should be behind VPN/firewall in production

## Escalation

1. Check `supervisorctl status` for crashed services
2. Check `docker logs agentbox` for container-level issues
3. Check disk space and memory: `docker stats agentbox`
4. If persistent: `docker compose restart agentbox`
