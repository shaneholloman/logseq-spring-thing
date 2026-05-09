# dreamlab-ai-website — Operational Runbook

| Field | Value |
|-------|-------|
| Substrate | dreamlab-ai-website |
| Repo | github.com/DreamLab-AI/dreamlab-ai-website |
| Runtime | Cloudflare Workers (downstream consumer of nostr-rust-forum kit) |
| Status | Pre-cutover (PRD-012, ADR-083) |

## Architecture

DreamLab's branded deployment of the nostr-bbs-rs forum kit:
- Inherits all workers from nostr-rust-forum (auth, relay, pod, search, preview)
- Custom `community-forum-rs/` overlay (being migrated to kit consumer model)
- Will gain `forum-config/` package per PRD-012 / ADR-085
- Static site content (landing pages, docs) served via Cloudflare Pages

## Deployment

```bash
# Current (pre-kit-adoption)
cd community-forum-rs && npx wrangler deploy --env production

# Future (post-kit-adoption per ADR-083)
# forum-config/ provides branded config; kit crates provide runtime
npx wrangler deploy --config forum-config/wrangler.toml --env production
```

## Health Checks

| Endpoint | Expected |
|----------|----------|
| GET / | 200 (landing page) |
| GET /community/health | 200 `{"status":"ok"}` |
| GET /.well-known/solid | 200 JSON-LD |

## Common Failure Modes

### Kit Version Mismatch
- **Symptom**: Build failure or runtime panic after kit update
- **Cause**: `forum-config/` references kit API that changed
- **Fix**: Check kit changelog, update `forum-config/` adapters. Run `cargo check --target wasm32-unknown-unknown`

### Cloudflare Pages Deploy Failure
- **Symptom**: Static content shows old version
- **Cause**: Build script error or cache issue
- **Fix**: Check CF Pages dashboard build logs; trigger manual redeploy

### Community Forum 500 Errors
- **Symptom**: Forum pages return 500
- **Cause**: Same as forum-runbook.md issues (D1, KV, DO)
- **Fix**: Follow forum-runbook.md troubleshooting

## Backup / Restore

Same as forum-runbook.md (inherits Cloudflare D1/KV/DO infrastructure).

Additional:
- **Static content**: Git-tracked, no separate backup needed
- **forum-config/**: Git-tracked configuration package
- **DNS/routing**: Cloudflare dashboard; document in separate infra runbook

## RTO / RPO Targets

| Component | RTO | RPO | Notes |
|-----------|-----|-----|-------|
| Static site | < 1 min | N/A | CF Pages CDN, auto-redeploy |
| Forum workers | < 1 min | N/A | Same as forum-runbook.md |
| D1 databases | < 15 min | < 24h | CF automatic backup |
| Custom config | N/A | N/A | Git-tracked |

## Cutover Plan (ADR-083)

The website is transitioning from `community-forum-rs/` monolith to kit consumer model:

1. **Current**: `community-forum-rs/` contains forked forum code
2. **Phase 1**: `forum-config/` package created with branded config (PRD-012)
3. **Phase 2**: Dual-deploy with traffic split (14-day window)
4. **Phase 3**: Cutover to kit + `forum-config/` as sole deployment
5. **Phase 4**: Delete `community-forum-rs/` (T₇+7 days after cutover)

During cutover, both old and new deployments run simultaneously. Parity monitoring compares response bodies.

## CI Pipeline

8-job pipeline validates:
- Rust build (wasm32 target)
- Clippy lints
- Format check
- Unit tests
- Integration tests
- Security audit
- Forum config validation
- Deploy preview (on PR)

## Monitoring

- Cloudflare Analytics dashboard
- `npx wrangler tail` per worker
- CI pipeline status badges in README

## Escalation

1. Check Cloudflare dashboard for platform issues
2. Follow forum-runbook.md for worker-level issues
3. Check `forum-config/` compatibility with kit version
4. File GitHub issue with logs and version info
