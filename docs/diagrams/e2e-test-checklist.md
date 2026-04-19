# VisionClaw End-to-End Test — Pristine Container Run

**Purpose**: validate the full pipeline after a clean rebuild with empty databases, against commit `1862c2d2f` (post diagnostic dial-back).

## Pre-conditions

- [x] Containers stopped via `./scripts/launch.sh down dev` or `compose down`
- [x] Volumes wiped: `visionflow-neo4j-data`, `visionflow-neo4j-logs`, `visionflow-data`, `visionflow-logs`
- [x] Build caches preserved: `visionflow-cargo-*`, `visionflow-npm-cache`
- [x] Diagnostic log gates removed (commit `1862c2d2f`)
- [x] RUST_LOG default demoted to `warn,webxr=info,...`
- [x] Rebuild triggered: `./scripts/launch.sh up dev`

## Layer 1 — Infrastructure

- [ ] `docker ps` shows `visionflow_container` (healthy), `visionflow-neo4j` (healthy), `visionflow-jss` (healthy or at least running)
- [ ] `visionflow-neo4j` reachable at port 7474 (HTTP) and 7687 (Bolt)
- [ ] `visionflow_container` serving on port 3001 (nginx) and 4000 (direct backend)
- [ ] No unexpected `warn!`/`error!` entries in first 60s of `docker logs visionflow_container`

## Layer 2 — Data Ingestion

- [ ] GitHub ontology sync kicks off on startup — look for `GithubSyncActor` logs
- [ ] Logseq pages processed into `KGNode` rows in Neo4j
- [ ] OWL ontology assembler → converter → Whelk reasoner pipeline executes
- [ ] Neo4j node count > 0 after ingestion (query: `MATCH (n) RETURN count(n)`)
- [ ] `iri_to_id` map populated (logs: `ONT-001: Built iri_to_id map — N KGNode nodes have owl_class_iri`)
- [ ] Ontology edges loaded (logs: `Loaded M ontology edges (SUBCLASS_OF + RELATES)`)

## Layer 3 — Real-time Pipeline

- [ ] WebSocket upgrade succeeds at `/wss` (101 Switching Protocols)
- [ ] V5 binary frames arrive at client — first byte is `0x05`, 9-byte header parsed
- [ ] `broadcast_sequence` increments monotonically
- [ ] No `BroadcastPositions#` diagnostic logs in server (confirms dial-back applied)
- [ ] Physics simulation running at ~60 Hz — `ForceComputeActor` emitting frames
- [ ] Client receives position updates — graph nodes animate in browser

## Layer 4 — Interactive

- [ ] Frontend loads at `http://localhost:3001` (no 5xx, no JS console errors)
- [ ] Graph renders with >0 nodes
- [ ] Sliders move the live graph (physics parameter changes apply without hard-refresh)
  - Attraction slider (0–10)
  - Dual Graph Separation (0–500)
  - Flatten to Planes (0–0.1)
- [ ] Enterprise drawer opens on Ctrl+Shift+E
- [ ] Settings PUT via enterprise drawer persists

## Layer 5 — Observability

- [ ] Log volume under `warn,webxr=info` is reasonable (not flooding)
- [ ] No boundary-stuck node rescues firing repeatedly (indicates stable physics)
- [ ] FastSettle either converges or falls back to Continuous cleanly
- [ ] `/api/health` returns healthy with physics simulation running

## Known-out-of-scope

- RuVector PostgreSQL NOT wiped (shared with other workspace projects — separate concern)
- Solid Pod data NOT wiped (`visionflow-jss-data` volume preserved)
- Build caches preserved (`visionflow-cargo-*`, `visionflow-npm-cache`)

## Rollback

If the E2E fails, the previous stable commit is `fcfc1a166` (the physics unblock commit before the documentation session). The logging change can be reverted with `git revert 1862c2d2f`.
