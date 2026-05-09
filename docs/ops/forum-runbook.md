# nostr-rust-forum (nostr-bbs-rs) — Operational Runbook

| Field | Value |
|-------|-------|
| Substrate | nostr-rust-forum |
| Product | nostr-bbs-rs (Cloudflare Workers forum kit) |
| Repo | github.com/DreamLab-AI/nostr-rust-forum |
| Version | 3.0.0-rc5 |
| Runtime | Cloudflare Workers (WASM) |

## Architecture

12 workspace crates compiled to 5 Workers + 1 WASM client:

| Worker | Crate | Purpose |
|--------|-------|---------|
| auth-worker | nostr-bbs-auth-worker | NIP-98 auth, WebAuthn, moderation admin |
| relay-worker | nostr-bbs-relay-worker | Nostr relay (NIP-01/11/42), Durable Objects |
| pod-worker | nostr-bbs-pod-worker | Solid Pod bridge, LDP/WAC |
| search-worker | nostr-bbs-search-worker | Full-text search, vector similarity |
| preview-worker | nostr-bbs-preview-worker | Link preview, OpenGraph |
| client | nostr-bbs-forum-client | Trunk-compiled WASM SPA |

Shared crates: nostr-bbs-core, nostr-bbs-config, nostr-bbs-rate-limit, nostr-bbs-mesh.

## Startup / Deployment

```bash
# Local development (requires wrangler)
cd crates/nostr-bbs-auth-worker && npx wrangler dev
cd crates/nostr-bbs-relay-worker && npx wrangler dev

# Production deploy (all workers)
npx wrangler deploy --env production  # per-worker
```

## Health Checks

| Endpoint | Worker | Expected |
|----------|--------|----------|
| GET /health | auth-worker | 200 `{"status":"ok"}` |
| GET /health | relay-worker | 200 `{"status":"ok"}` |
| GET /.well-known/solid | pod-worker | 200 JSON-LD |
| GET /health | search-worker | 200 `{"status":"ok"}` |

## Common Failure Modes

### D1 Database Errors
- **Symptom**: 500 errors on auth/moderation endpoints
- **Cause**: D1 binding not configured or migration not applied
- **Fix**: `npx wrangler d1 migrations apply <DB_NAME> --env production`

### Rate Limit Exhaustion
- **Symptom**: 429 responses on all endpoints
- **Cause**: nostr-bbs-rate-limit token bucket depleted
- **Fix**: Check KV binding `RATE_LIMIT` exists; increase `burst` in config

### WebAuthn Registration Failure
- **Symptom**: Passkey creation fails silently
- **Cause**: RP ID mismatch between config and origin
- **Fix**: Verify `WEBAUTHN_RP_ID` matches the deployment domain

### Relay Durable Object Crash
- **Symptom**: WebSocket connections drop, relay unresponsive
- **Cause**: DO storage limit or uncaught panic in nip_handlers
- **Fix**: `npx wrangler tail relay-worker` for logs; restart DO via alarm reset

## Backup / Restore

- **D1 databases**: Cloudflare automatic backups (30-day retention). Manual: `npx wrangler d1 export <DB>`
- **KV namespaces**: No native backup. Use `wrangler kv:key list` + bulk export script
- **Durable Objects**: State is per-object. Use DO alarm-based checkpoint if critical

## RTO / RPO Targets

| Component | RTO | RPO | Notes |
|-----------|-----|-----|-------|
| Workers (stateless) | < 1 min | N/A | Auto-redeploy from CI |
| D1 (auth state) | < 15 min | < 24h | CF automatic backup |
| KV (sessions, config) | < 5 min | < 1h | Recreate from config |
| Durable Objects (relay) | < 5 min | Last alarm checkpoint | Event replay from relay log |

## Monitoring

- Cloudflare Dashboard → Workers → Analytics (request count, error rate, latency)
- `npx wrangler tail <worker>` for real-time log streaming
- CI: 7-job pipeline validates build, test, audit on every push

## Escalation

1. Check `npx wrangler tail` logs
2. Check Cloudflare status page for platform issues
3. If D1/DO issue: file Cloudflare support ticket
4. If code issue: create GitHub issue with `npx wrangler tail` output
