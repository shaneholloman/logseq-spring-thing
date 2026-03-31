# Turbo Flow Container - Claude Flow V3

> **Hierarchy**: This file inherits from `../CLAUDE.md` (project root) which
> inherits from `../../CLAUDE.md` (workspace). Those parent files define:
> memory-first protocol, intelligent skill selection, agent routing, tool
> delineation, and behavioural rules. This file adds container-specific
> configuration only. Do not duplicate parent content here.
>
> **If parent CLAUDE.md files are not visible** (running from a subdirectory
> without the project mount): the essential rules are: (1) ALWAYS use
> `mcp__claude-flow__memory_*` MCP tools for memory, never CLI, never raw SQL
> (raw SQL bypasses embedding generation — entries become invisible to semantic search); (2) check
> `SKILL-DIRECTORY.md` in skills/ for routing tasks to the right skill;
> (3) batch all operations in one message; (4) read before edit; (5) never
> save to root directories.

## EXTERNAL MEMORY SYSTEM (PRIMARY — MANDATORY)

The external RuVector PostgreSQL container is the **primary store and recall** for all agentic orchestration.
All agents, swarms, and sessions share this single persistent memory. It survives container rebuilds.

### Connection
| Property | Value |
|----------|-------|
| Host | `ruvector-postgres` (docker_ragflow network) |
| Port | `5432` |
| Database | `ruvector` |
| User | `ruvector` |
| ConnInfo | `$RUVECTOR_PG_CONNINFO` |
| Extension | RuVector v2.0.0 (112 SQL functions, AVX-512 SIMD) |
| Vector dims | 384 (all-MiniLM-L6-v2, client-side ONNX) |
| Index | HNSW (m=16, ef_construction=64) — sub-millisecond cosine search |

### Store and Recall via MCP (preferred)
```javascript
// STORE — after any successful task, pattern discovery, or decision
mcp__claude-flow__memory_store({
  namespace: "patterns",     // or: coordination, tasks, agent-assignments, hooks:post-task
  key: "descriptive-key",
  value: JSON.stringify({description: "what worked", category: "...", confidence: 0.9})
})

// RECALL — before starting any task
mcp__claude-flow__memory_search({query: "[task keywords]", namespace: "patterns", limit: 10})
```

### Store and Recall via SQL (advanced — for bulk ops, cross-namespace search, analytics)
```bash
# Store
PGPASSWORD=ruvector psql -h ruvector-postgres -U ruvector -d ruvector -c "
INSERT INTO memory_entries (id, project_id, namespace, key, value, metadata, source_type)
VALUES (gen_random_uuid()::text, <project_id>, '<namespace>', '<key>', '<json>'::jsonb, '<meta>'::jsonb, 'claude')
ON CONFLICT ON CONSTRAINT memory_entries_pkey DO UPDATE SET value = EXCLUDED.value, updated_at = now();
"

# Recall by vector similarity (HNSW — requires embedding column populated)
# Use count(id) not count(*) — extension quirk with count(*)
PGPASSWORD=ruvector psql -h ruvector-postgres -U ruvector -d ruvector -c "
WITH query AS (SELECT embedding FROM memory_entries WHERE key = '<known-key>')
SELECT me.key, me.namespace, me.value, (1 - (me.embedding <=> q.embedding))::numeric(6,4) as similarity
FROM memory_entries me, query q
WHERE me.embedding IS NOT NULL
ORDER BY me.embedding <=> q.embedding LIMIT 10;
"

# Recall by namespace + JSONB content
PGPASSWORD=ruvector psql -h ruvector-postgres -U ruvector -d ruvector -c "
SELECT key, value->>'description' FROM memory_entries
WHERE namespace = 'patterns' AND value->>'category' = 'architecture'
ORDER BY updated_at DESC LIMIT 20;
"
```

### Database Schema (6 tables)
| Table | Purpose | Key columns |
|-------|---------|-------------|
| `memory_entries` | Primary KV + vector store | `key`, `namespace`, `value` (JSONB), `embedding` ruvector(384), `project_id`, `agent_id` |
| `projects` | Project registry (20 projects) | `name`, `path`, `git_remote`, `total_entries` |
| `patterns` | Learned code/workflow patterns | `type`, `pattern`, `confidence`, `embedding` ruvector(384) |
| `reasoning_patterns` | ReasoningBank trajectories | `pattern_key`, `confidence`, `success_count`, `failure_count` |
| `sona_trajectories` | SONA self-optimization tracking | `trajectory_id`, `agent_id`, `steps` (JSONB), `success` |
| `session_state` | Session persistence | `session_id`, `state` (JSONB), `agents`, `tasks` |

### Extension Capabilities (beyond vector search)
- **Cypher graph queries**: `SELECT ruvector_cypher('graph_name', 'MATCH (n) RETURN n', '{}')`
- **SPARQL**: `SELECT ruvector_sparql('store_name', 'SELECT ?s ?p ?o WHERE { ?s ?p ?o }', '{}')`
- **Agent routing**: `ruvector_register_agent()`, `ruvector_route()`, `ruvector_find_agents_by_capability()`
- **Self-learning**: `ruvector_enable_learning()`, `ruvector_record_feedback()`, `ruvector_extract_patterns()`
- **Graph ops**: `ruvector_add_node()`, `ruvector_add_edge()`, `ruvector_shortest_path()`, `ruvector_cypher()`
- **Attention**: `attention_score()`, `attention_softmax()`, `attention_weighted_add()`
- **Hyperbolic geometry**: `ruvector_poincare_distance()`, `ruvector_lorentz_distance()`, `ruvector_mobius_add()`
- **Temporal**: `temporal_velocity()`, `temporal_drift()`, `temporal_ema_update()`

### Mandatory Protocol
1. **NEVER** use `claude-flow memory *` CLI commands — they bypass the external store
2. **ALWAYS** use MCP memory tools (`mcp__claude-flow__memory_*`) for standard ops
3. Use SQL only for bulk operations, analytics, or cross-namespace vector search
4. Embeddings are generated **client-side** by claude-flow's ONNX runtime (all-MiniLM-L6-v2)
5. The `ruvector_embed()` SQL function is NOT available in this image — do not call it
6. Use `count(id)` not `count(*)` when querying row counts (extension aggregate quirk)

## V4 Three-Tier Memory Protocol

### Session Start
1. Run `bd ready` to check project state (blockers, in-progress work, decisions)
2. Recall from external memory: `mcp__claude-flow__memory_search({query: "[project context]", limit: 10})`
3. Check Native Tasks from prior sessions

### During Work — Decision Tree
- **Project roadmap / blockers / dependencies / decisions** → `bd add` (Beads)
- **Current session tasks / active checklist** → Native Tasks
- **Learned patterns / routing weights / skills** → External memory (store via MCP, persists in RuVector PG)
- **Cross-agent coordination state** → External memory `coordination` namespace
- **Agent assignment history** → External memory `agent-assignments` namespace

### Session End
- File discovered work as Beads issues: `bd add --type issue "description"`
- Summarize architectural decisions: `bd add --type decision "description"`
- Store session outcomes: `mcp__claude-flow__memory_store({namespace: "patterns", key: "session-<date>-summary", value: "..."})`
- External memory persists automatically across container rebuilds

## Agent Isolation via Git Worktrees

Each parallel agent MUST operate in its own git worktree to prevent file conflicts.

### Three ways to use worktrees

| Method | When | Command |
|--------|------|---------|
| **Claude Code native** | Interactive session needing isolation | `claude -w` (auto-creates worktree) |
| **Agent tool** | Spawning subagents from orchestrator | `Agent({ isolation: "worktree", ... })` |
| **Custom (Ruflo swarms)** | Multi-agent swarm with PG schema isolation | `wt-add <agent-name>` |
| **Batch fan-out** | Massive parallelisable changesets | `/batch` (interviews, then fans to N worktree agents) |

### `wt-add` / `wt-remove` (container-optimised)
```bash
wt-add <agent-name>    # Creates .worktrees/<name>, branch <name>/<timestamp>, PG schema, GitNexus index
wt-remove <agent-name> # Cleans up worktree + branch
```

### Agent tool worktree isolation
When spawning subagents, use `isolation: "worktree"` for automatic worktree creation and cleanup:
```
Agent({ description: "Fix auth bug", prompt: "...", isolation: "worktree" })
```
The worktree is auto-cleaned if the agent makes no changes. If changes are made, the worktree path and branch are returned in the result for merge.

### Best practices
- Use worktrees for ANY task touching 2+ files that another agent might also touch
- Ruflo swarm agents should ALWAYS use `wt-add` (provides PG schema + GitNexus)
- Agent tool subagents should use `isolation: "worktree"` (lighter weight, auto-cleanup)
- `/batch` for large migrations — fans out to dozens/hundreds of worktree agents automatically
- NEVER run `--dangerously-skip-permissions` on bare metal — containers only

## Agent Teams
- `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` is enabled
- Lead agent may spawn up to 3 teammates (each gets own worktree via `claude -w`)
- Recursion limit: depth 2
- If 3+ agents are blocked simultaneously → pause and alert human

## Codebase Intelligence (GitNexus)
- Index repo: `gnx-analyze` (creates knowledge graph)
- Before editing shared code: check blast radius via GitNexus
- Auto-indexes new worktrees on `wt-add`

## Cost Guardrails
- Monitor: `claude-usage` or ruflo statusline
- Use Haiku for simple tasks — don't burn Opus on formatting

## Guidance Control Plane (Automatic)

Compiled via hooks: SessionStart compiles CLAUDE.md into typed constitution, UserPromptSubmit retrieves task-scoped shards, PreToolUse checks enforcement gates.

```bash
claude-flow guidance compile        # Compile policy
claude-flow guidance retrieve --task "implement auth"
claude-flow guidance status
```

Use `CLAUDE.local.md` for experiments. Promote with ADR when validated.

## Claude Cowork (Desktop Cowork Mode on Linux)
Claude Desktop's Cowork mode runs natively on Linux via Electron + stub modules.
Source: `johnzfitch/claude-cowork-linux` | Install: `~/.local/share/claude-desktop/`

| Command | Action |
|---------|--------|
| `cowork start` | Launch on VNC Display :1 |
| `cowork stop` | Stop all processes |
| `cowork restart` | Restart |
| `cowork status` | Check if running |
| `cowork logs` | View startup logs |
| `claude-desktop --devtools` | Launch with DevTools |
| `claude-desktop --doctor` | Run diagnostics |

Requires Claude Pro (or higher) subscription. Accessible via VNC on port 5901.

## Container Environment

### Multi-User System
| User | UID | Purpose | Switch |
|------|-----|---------|--------|
| devuser | 1000 | Claude Code, primary dev | - |
| gemini-user | 1001 | Google Gemini, gemini-flow | `as-gemini` |
| openai-user | 1002 | OpenAI Codex | `as-openai` |
| zai-user | 1003 | Z.AI service (port 9600) | `as-zai` |
| deepseek-user | 1004 | DeepSeek API | `as-deepseek` |
| local-private | 1005 | Private LLM (Nemotron 3 120B) | `as-local` |

### Service Ports
| Port | Service | Access |
|------|---------|--------|
| 22 | SSH | Public (mapped to 2222) |
| 5901 | VNC | Public |
| 8080 | code-server | Public |
| 9090 | Management API | Public |
| 9600 | Z.AI | Internal only |
| 3100 | Local LLM Proxy | Internal only |

### Local LLM Proxy (Nemotron 3 120B)
Agentic-flow translates Anthropic API → OpenAI format for llama.cpp at `192.168.2.48:8080`.
```bash
llm-proxy-start                    # Start proxy (supervisord)
llm-proxy-status                   # Check proxy health
as-local                           # Switch to local-private user (Claude CLI auto-routed)
curl http://localhost:3100/health   # Direct health check
```
The `local-private` user has `ANTHROPIC_BASE_URL=http://localhost:3100` pre-configured.
Claude CLI in that user context routes through the proxy to Nemotron automatically.

### 610 Sub-Agents
`/home/devuser/agents/*.md` -- Load: `cat agents/<name>.md`
Key: doc-planner, microtask-breakdown, github-pr-manager, tdd-london-swarm

### Z.AI Service
Port 9600 (internal) | Workers: 4 concurrent
```bash
curl http://localhost:9600/health
curl -X POST http://localhost:9600/chat -H "Content-Type: application/json" -d '{"prompt": "...", "timeout": 30000}'
```

### Gemini Flow
`gf-init`, `gf-swarm` (66 agents), `gf-architect`, `gf-coder`, `gf-status`, `gf-monitor`, `gf-health`

### tmux Workspace
`tmux attach -t workspace` -- 8 windows: Claude-Main(0), Claude-Agent(1), Services(2), Dev(3), Logs(4), System(5), VNC(6), SSH(7)

### Management API
`http://localhost:9090` | Auth: `X-API-Key: <MANAGEMENT_API_KEY>`
Endpoints: GET /health, GET /api/status, POST /api/tasks, GET /api/tasks/:id, GET /metrics

### Diagnostics
```bash
sudo supervisorctl status              # Service status
tail -f /var/log/supervisord.log       # Logs
docker exec turbo-flow-unified supervisorctl status  # From host
```

### Container Modification
- DO: Edit `multi-agent-docker/` files directly in the project -- fix root cause
- DON'T: Patching scripts or workarounds
- Validate: `cargo test`, `npm test`, `pytest`
- Container is isolated from external build systems

**Security** (DEV ONLY): SSH `devuser:turboflow` | VNC `turboflow` | API `change-this-secret-key`

