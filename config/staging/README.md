# VisionClaw — Staging Environment

Self-contained compose stack for exercising the sovereign-mesh plane in
an environment that mirrors production routing but keeps state local.
Exposes a full observability loop: Prometheus scrapes the 24 sovereign-mesh
metrics every 15 s; Grafana ships with the `Sovereign Mesh — Overview`
dashboard pre-provisioned.

## Quick start

```bash
# From repo root. Put secrets in .env.staging first (see below).
docker compose -f config/staging/docker-compose.staging.yml \
  --env-file .env.staging up -d

# Tail logs
docker compose -f config/staging/docker-compose.staging.yml logs -f visionclaw

# Reset all state (wipes volumes):
docker compose -f config/staging/docker-compose.staging.yml down -v
```

## .env.staging

Create `.env.staging` in the repo root with at least:

```dotenv
NEO4J_PASSWORD=staging-neo4j-change-me
GRAFANA_ADMIN_USER=admin
GRAFANA_ADMIN_PASSWORD=staging-grafana-change-me
SERVER_NOSTR_PRIVKEY=<hex-64-char privkey for server identity>
# Optional forum relay
VISIONCLAW_NOSTR_PRIVKEY=
FORUM_RELAY_URL=
```

Do not reuse production keys. This file is gitignored.

## Port map

| Service     | Host port | Container port | URL                          |
|-------------|-----------|----------------|------------------------------|
| visionclaw  | 4000      | 4000           | http://localhost:4000        |
| neo4j HTTP  | 7474      | 7474           | http://localhost:7474        |
| neo4j Bolt  | 7687      | 7687           | bolt://localhost:7687        |
| jss (Solid) | 3030      | 3030           | http://localhost:3030        |
| prometheus  | 9090      | 9090           | http://localhost:9090        |
| grafana     | 3000      | 3000           | http://localhost:3000        |

Prometheus scrape target: `http://visionclaw:4000/metrics` (inside the
`visionclaw-staging` docker network).

## Grafana

Default URL: http://localhost:3000
Default user/password: from `GRAFANA_ADMIN_USER` / `GRAFANA_ADMIN_PASSWORD`
in `.env.staging`.

Open the folder `VisionClaw` → dashboard `Sovereign Mesh — Overview` to
see all 24 metrics visualised. The datasource is provisioned read-only
(`editable: false`) so accidental changes cannot break scraping.

## Reset state

```bash
docker compose -f config/staging/docker-compose.staging.yml down -v
```

`-v` removes Neo4j, Solid, Prometheus and Grafana volumes. The first
bring-up after a reset takes ~1 min while Neo4j downloads APOC+GDS plugins.

## Feature flags enabled in staging

`METRICS_ENABLED=true`, `POD_SAGA_ENABLED=true`, `BRIDGE_EDGE_ENABLED=true`,
`NIP98_OPTIONAL_AUTH=true`. Override individually via `.env.staging` if a
scenario requires them off.
