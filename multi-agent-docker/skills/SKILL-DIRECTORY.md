# Skill Directory -- Comprehensive Inventory and Decision Tree

> **72 skills** audited. **6 deprecated**, **9 archived**, **57 active**.
> Generated 2026-03-25. Reference this file from CLAUDE.md for intelligent routing.

---

## Deprecated and Archived Skills (DO NOT USE)

| Skill | Status | Replacement |
|-------|--------|-------------|
| `agentdb-optimization` | DEPRECATED | `agentdb-vector-search` |
| `pair-programming` | DEPRECATED | `build-with-quality` |
| `perplexity` | DEPRECATED | `perplexity-research` |
| `reasoningbank-agentdb` | DEPRECATED | `build-with-quality` |
| `reasoningbank-intelligence` | DEPRECATED | `build-with-quality` |
| `swarm-orchestration` | DEPRECATED | `swarm-advanced` |
| `v3-cli-modernization` | ARCHIVED | Reference only (v3 shipped) |
| `v3-core-implementation` | ARCHIVED | Reference only (v3 shipped) |
| `v3-ddd-architecture` | ARCHIVED | Reference only (v3 shipped) |
| `v3-integration-deep` | ARCHIVED | Reference only (v3 shipped) |
| `v3-mcp-optimization` | ARCHIVED | Reference only (v3 shipped) |
| `v3-memory-unification` | ARCHIVED | Reference only (v3 shipped) |
| `v3-performance-optimization` | ARCHIVED | Reference only (v3 shipped) |
| `v3-security-overhaul` | ARCHIVED | Reference only (v3 shipped) |
| `v3-swarm-coordination` | ARCHIVED | Reference only (v3 shipped) |

---

## Artefact 1: Categorised Skill Inventory (56 Active Skills)

### Context, Discovery, and Session Management

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `lazy-fetch` | Yes | 25 MCP tools: context hydration, plan tracking, blueprints, PRD-to-sprints, security scanning, persistent memory | Starting a new session, managing context across tasks, tracking phased plans, running autonomous PRD execution |
| `skill-builder` | No | Create new Claude Code skills with YAML frontmatter and progressive disclosure | Building new custom skills for the skills directory |

### Development Methodology and Meta-Skills

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `build-with-quality` | No | 111+ agents, unified dev+QE, TDD, ADR, quality gates, SONA learning, TinyDancer routing. **Supersedes** pair-programming, reasoningbank-*, agentic-qe | Any multi-file feature, refactor, or project needing quality gates, coverage, and testing |
| `sparc-methodology` | No | SPARC 5-phase development (Specification, Pseudocode, Architecture, Refinement, Completion), 17 modes | Systematic multi-phase development from spec through deployment |
| `prd2build` | No | PRD to complete documentation in a single command | Generating full project documentation from a product requirements document |

### Code Quality, Review, and Verification

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `github-code-review` | No | Multi-agent AI code review, security/performance analysis, quality gates | Reviewing PRs on GitHub with specialised review agents |
| `verification-quality` | No | Truth scoring (0.0-1.0), automatic rollback at 0.95 threshold, CI/CD export | Ensuring code correctness with truth-score verification and auto-rollback |
| `docs-alignment` | No | 15-agent swarm for documentation validation, Diataxis framework, link coverage, Mermaid diagrams | Validating and modernising project documentation against codebase |

### Testing and QA

Testing is integrated into `build-with-quality` (TDD agents) and `sparc-methodology` (TDD mode). No standalone testing skill exists. For truth-score verification, use `verification-quality`.

### GitHub and CI/CD

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `github-code-review` | No | Swarm-coordinated PR review, security/performance analysis | Reviewing pull requests |
| `github-multi-repo` | No | Cross-repo coordination, package sync, architecture management | Managing multiple repositories in an organisation |
| `github-project-management` | No | Issue tracking, project boards, sprint planning with swarm agents | Agile project management on GitHub |
| `github-release-management` | No | Automated versioning, changelog, multi-platform deployment, rollback | Cutting releases and managing deployments |
| `github-workflow-automation` | No | GitHub Actions CI/CD pipeline creation and management | Creating or modifying GitHub Actions workflows |

### Swarm and Multi-Agent Orchestration

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `swarm-advanced` | No | Advanced swarm patterns: mesh, hierarchical, adaptive topologies, fault tolerance | Local multi-agent orchestration for research, dev, and testing |
| `hive-mind-advanced` | No | Queen-led hierarchical coordination, Byzantine consensus, collective memory | When you need consensus-driven multi-agent decisions |
| `flow-nexus-swarm` | No | Cloud-based AI swarm deployment, event-driven workflows, message queues | Deploying swarms on Flow Nexus cloud infrastructure |
| `stream-chain` | No | Sequential multi-agent pipelines where output chains between steps | When each agent's output feeds the next (sequential, not parallel) |
| `hooks-automation` | No | Pre/post task hooks, session management, Git hooks, neural training | Automating development operations with intelligent hooks |

### Memory, Learning, and Intelligence

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `agentdb-advanced` | No | QUIC sync, multi-database, custom distance metrics, hybrid search, MMR diversity | Distributed AI systems needing cross-network AgentDB sync |
| `agentdb-learning` | No | 9 RL algorithms (Decision Transformer, Q-Learning, SARSA, Actor-Critic, etc.) | Building self-learning agents with reinforcement learning |
| `agentdb-memory-patterns` | No | Session memory, long-term storage, pattern learning, context management | Stateful agents, chat systems, intelligent assistants |
| `agentdb-vector-search` | No | HNSW indexing (150x faster), quantization, caching, batch ops, RAG pipelines | Semantic search, RAG systems, scaling to millions of vectors |

### AI/ML and Neural Networks

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `pytorch-ml` | No | PyTorch, CUDA GPU, data science stack, distributed training | Deep learning model training and research |
| `cuda` | Yes | 4 specialist agents, kernel optimisation, compilation, GPU profiling | Custom CUDA kernel development and GPU programming |
| `flow-nexus-neural` | Yes | Distributed neural network training in E2B sandboxes (feedforward, LSTM, GAN, transformer) | Training models in cloud sandboxes via Flow Nexus |
| `deepseek-reasoning` | Yes | DeepSeek special model endpoint, structured chain-of-thought, multi-step reasoning | Complex reasoning tasks requiring DeepSeek's reasoning model |
| `openai-codex` | Yes | GPT-5.4 code generation and review via MCP bridge | Delegating specific tasks to GPT-5.4 capabilities |

### Browser Automation and Web

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `browser-automation` | No | **Meta-skill**: decision framework for choosing between the 4 browser tools below | Unsure which browser tool to use -- start here |
| `browser` | No | agent-browser with AI-optimised snapshots, 93% context reduction via @refs | Quick form filling, scraping, navigation with minimal context |
| `playwright` | Yes | Full Playwright API, screenshots, visual testing on Display :1 via VNC | Visual testing, complex automation, screenshot verification |
| `chrome-cdp` | No | CDP CLI for live Chromium sessions, 100+ tabs, no Puppeteer dependency | Inspecting already-open browser tabs, logged-in sessions |
| `host-webserver-debug` | Yes | HTTPS-to-HTTP bridge for debugging host web servers from Docker | Cross-origin/CORS issues when accessing host dev servers |

### Web Research and Content

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `perplexity-research` | No | Real-time web search via Perplexity API with citations | Current information, market research, live web data |
| `gemini-url-context` | Yes | Gemini 2.5 Flash URL expansion, up to 20 URLs per request, grounding metadata | Analysing or summarising specific known URLs |
| `web-summary` | Yes | URL summarisation, YouTube transcript extraction, Logseq/Obsidian topic links | Summarising articles, YouTube videos, generating note links |

### Documentation and Reports

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `report-builder` | No | LaTeX reports, Python analytics, Wardley maps, TikZ+Mermaid diagrams, multi-LLM research | Research reports, white papers, sector analyses, policy briefs |
| `latex-documents` | No | TeX Live toolchain, Beamer presentations, BibTeX, mathematical typesetting | Academic papers, presentations, publication-quality documents |
| `mermaid-diagrams` | No | 25 diagram types, PNG/SVG/PDF export, dark/light themes | System architecture, flowcharts, ER models, Gantt charts, mindmaps |
| `paperbanana` | No | Publication-quality academic figures via multi-agent VLM pipeline (Gemini/OpenAI) | Research paper figures, methodology diagrams, statistical plots |
| `fossflow` | No | Isometric network/architecture diagrams, compact LLM-optimised format | Network topology diagrams and infrastructure maps |

### UI/UX Design

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `ui-ux-pro-max-skill` | No | 50 styles, 97 palettes, 57 font pairings, 9 tech stacks, shadcn/ui MCP | Designing UI components, choosing palettes/typography, reviewing UX |
| `daisyui` | No | daisyUI 5 components, theme configuration, Tailwind CSS patterns | Building web interfaces specifically with daisyUI components |

### Media Processing

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `ffmpeg-processing` | No | FFmpeg 8.0: transcode, edit, stream, filter, HLS/DASH/RTMP, CUDA acceleration | Video/audio conversion, editing, streaming, batch processing |
| `imagemagick` | Yes | Format conversion, resize, crop, filter, batch ops, watermarks, metadata | Image format conversion, thumbnails, batch image processing |
| `comfyui` | Yes | Stable Diffusion, FLUX, node-based workflows, distributed GPU (Salad Cloud) | AI image/video generation from prompts or workflows |

### 3D and Game Development

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `game-dev` | No | 48 agents, 38 commands across Godot/Unity/Unreal, design/programming/art/audio/QA | Full game development projects across any major engine |
| `terracraft` | No | OSM + elevation + arnis pipeline: real-world locations to Minecraft Java worlds. QGIS/Blender/GDAL integration | Generating Minecraft worlds from real geography, geospatial-to-game conversion |
| `blender` | No | 3D modelling, scene creation, rendering, material PBR, import/export via socket | Programmatic 3D modelling, automated Blender workflows |

### Geospatial

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `qgis` | Yes | 51 MCP tools: layer management, geoprocessing, rendering, styling, CRS transforms | GIS operations, geospatial analysis, map generation |

### Version Control (AI-Native)

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `agentic-jujutsu` | No | Lock-free VCS (23x faster than Git), auto conflict resolution (87%), multi-agent coordination | Multiple AI agents modifying code simultaneously |

### Ontology and Knowledge Graphs

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `ontology-core` | No | Logseq ontology parsing, OWL2 DL TTL export, WebVOWL compatibility | Creating new ontology schemas from Logseq data |
| `ontology-enrich` | No | Validation, enrichment, TTL generation for existing ontology data | Enriching or validating existing ontology datasets |

### Platform Management

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `flow-nexus-platform` | No | Authentication, sandboxes, app deployment, payments, challenges on Flow Nexus | Managing Flow Nexus accounts, sandboxes, and deployments |

### Systems Programming

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `rust-development` | No | Cargo, rustfmt, clippy, rust-analyzer, WASM compilation, cross-compilation | Rust systems programming, CLI tools, network services |
| `wasm-js` | No | High-performance WASM graphics, JS interop, Canvas/WebGL with WASM compute | Performance-critical web graphics, real-time animations, hybrid JS/WASM |

### Performance

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `performance-analysis` | No | Bottleneck detection, profiling, reporting, optimisation recommendations for swarms | Profiling swarm operations and identifying performance issues |

### Data Science and Notebooks

| Skill | MCP | Key Capability | When to Choose |
|-------|-----|----------------|----------------|
| `jupyter-notebooks` | No | Full Jupyter MCP: create, execute, manipulate cells, access outputs | Interactive data analysis, ML experiments, reproducible research |

---

## Artefact 2: Decision Tree

Answer these questions in order. Stop at the first match.

```
Q1: Is the task about an EXISTING skill that is deprecated?
    YES --> Use its replacement (see Deprecated table above)

Q2: What is the primary domain?

    [A] CODE DEVELOPMENT (write, refactor, test, review code)
    [B] GITHUB OPERATIONS (PRs, issues, releases, workflows, multi-repo)
    [C] MULTI-AGENT / SWARM COORDINATION
    [D] RESEARCH, WEB, or CONTENT (fetch URLs, search web, summarise)
    [E] DOCUMENTS and REPORTS (LaTeX, diagrams, docs validation)
    [F] MEDIA (image, video, audio, 3D, AI generation)
    [G] BROWSER AUTOMATION (scrape, test UI, debug web apps)
    [H] AI/ML MODEL WORK (train, deploy, reason)
    [I] MEMORY and LEARNING (store, search, vector DB, RL)
    [J] INFRASTRUCTURE / PLATFORM / DEVOPS
    [K] DOMAIN-SPECIFIC (GIS, ontology, game dev, UI design)
```

### [A] Code Development

```
Q3: How complex is the task?
    |
    +-- Single file, quick fix --> Edit directly (no skill needed)
    |
    +-- Multi-file feature with quality gates, TDD, coverage
    |   --> build-with-quality
    |
    +-- Systematic multi-phase (spec -> arch -> code -> deploy)
    |   --> sparc-methodology
    |
    +-- Just need truth-score verification and rollback safety
    |   --> verification-quality
    |
    +-- Rust systems programming
    |   --> rust-development
    |
    +-- WASM + JS graphics interop
    |   --> wasm-js
    |
    +-- CUDA GPU kernel development
    |   --> cuda
    |
    +-- Generate docs from PRD (no code implementation)
        --> prd2build
```

### [B] GitHub Operations

```
Q3: What GitHub operation?
    |
    +-- Review a PR with AI agents
    |   --> github-code-review
    |
    +-- Cut a release, changelog, deploy
    |   --> github-release-management
    |
    +-- Create/modify GitHub Actions workflows
    |   --> github-workflow-automation
    |
    +-- Manage issues, project boards, sprints
    |   --> github-project-management
    |
    +-- Cross-repo sync, architecture, org-wide automation
        --> github-multi-repo
```

### [C] Multi-Agent / Swarm Coordination

```
Q3: What coordination pattern?
    |
    +-- Local swarm with parallel agents (mesh, hierarchical, adaptive)
    |   --> swarm-advanced
    |
    +-- Queen-led consensus with Byzantine fault tolerance
    |   --> hive-mind-advanced
    |
    +-- Cloud-based swarm on Flow Nexus
    |   --> flow-nexus-swarm
    |
    +-- Sequential pipeline (output of step N feeds step N+1)
    |   --> stream-chain
    |
    +-- Automated hooks for pre/post operations and session management
    |   --> hooks-automation
    |
    +-- Multiple AI agents editing code simultaneously (lock-free VCS)
        --> agentic-jujutsu
```

### [D] Research, Web, and Content

```
Q3: What do you need?
    |
    +-- Live web search with citations
    |   --> perplexity-research
    |
    +-- Analyse/summarise specific URLs (up to 20)
    |   --> gemini-url-context
    |
    +-- Summarise articles or YouTube videos for notes
    |   --> web-summary
    |
    +-- Delegate complex reasoning to DeepSeek
    |   --> deepseek-reasoning
    |
    +-- Delegate coding/review to GPT-5.4
        --> openai-codex
```

### [E] Documents and Reports

```
Q3: What kind of document?
    |
    +-- Full research report / white paper / policy brief
    |   --> report-builder
    |
    +-- Academic paper, Beamer presentation, math typesetting
    |   --> latex-documents
    |
    +-- Diagrams only (flowchart, ER, sequence, Gantt, mindmap)
    |   --> mermaid-diagrams
    |
    +-- Publication-quality academic figures (methodology, stats plots)
    |   --> paperbanana
    |
    +-- Isometric network / infrastructure diagrams
    |   --> fossflow
    |
    +-- Validate existing docs against codebase
    |   --> docs-alignment
    |
    +-- Generate project docs from a PRD
        --> prd2build
```

### [F] Media Processing

```
Q3: What media type?
    |
    +-- Video/audio transcode, edit, stream
    |   --> ffmpeg-processing
    |
    +-- Image format conversion, resize, crop, batch processing
    |   --> imagemagick
    |
    +-- AI image/video generation (Stable Diffusion, FLUX)
    |   --> comfyui
    |
    +-- 3D modelling and rendering
    |   --> blender
    |
    +-- Game assets and full game dev pipeline
        --> game-dev
```

### [G] Browser Automation

```
Q3: Do you know which tool you need?
    |
    +-- No / unsure --> browser-automation (meta-skill, guides selection)
    |
    +-- Quick scrape/form-fill with minimal context
    |   --> browser (agent-browser, @ref snapshots)
    |
    +-- Full API, screenshots, visual testing on Display :1
    |   --> playwright
    |
    +-- Inspect live Chromium tabs already open
    |   --> chrome-cdp
    |
    +-- Debug host web server from inside Docker (CORS/HTTPS)
        --> host-webserver-debug
```

### [H] AI/ML Model Work

```
Q3: What ML task?
    |
    +-- PyTorch model training (local GPU)
    |   --> pytorch-ml
    |
    +-- Custom CUDA kernels
    |   --> cuda
    |
    +-- Distributed training in cloud sandboxes
    |   --> flow-nexus-neural
    |
    +-- Interactive notebook-based experiments
    |   --> jupyter-notebooks
    |
    +-- Self-learning agents with RL algorithms
        --> agentdb-learning
```

### [I] Memory and Learning

```
Q3: What memory operation?
    |
    +-- Session/long-term agent memory patterns
    |   --> agentdb-memory-patterns
    |
    +-- Semantic vector search, RAG, HNSW tuning
    |   --> agentdb-vector-search
    |
    +-- Distributed multi-DB sync, QUIC, hybrid search
    |   --> agentdb-advanced
    |
    +-- Reinforcement learning plugins
    |   --> agentdb-learning
    |
    +-- Session context and plan tracking
        --> lazy-fetch
```

### [J] Infrastructure / Platform / DevOps

```
Q3: What infrastructure task?
    |
    +-- Flow Nexus account, sandbox, app deployment, payments
    |   --> flow-nexus-platform
    |
    +-- Swarm performance profiling and bottleneck detection
    |   --> performance-analysis
    |
    +-- GitHub Actions CI/CD pipelines
    |   --> github-workflow-automation
    |
    +-- Creating new skills for this system
        --> skill-builder
```

### [K] Domain-Specific Tools

```
Q3: Which domain?
    |
    +-- Geospatial analysis, GIS, maps
    |   --> qgis
    |
    +-- Ontology creation (Logseq -> OWL2 TTL)
    |   --> ontology-core
    |
    +-- Ontology validation and enrichment
    |   --> ontology-enrich
    |
    +-- Game development (Godot/Unity/Unreal)
    |   --> game-dev
    |
    +-- UI/UX design (styles, palettes, fonts, accessibility)
    |   --> ui-ux-pro-max-skill
    |
    +-- daisyUI components specifically
        --> daisyui
```

---

## Skill Composition Patterns

Some tasks benefit from combining skills. Common compositions:

| Task Pattern | Primary Skill | Supporting Skill(s) |
|--------------|---------------|---------------------|
| Feature with tests and PR review | `build-with-quality` | `github-code-review` |
| Research report with diagrams | `report-builder` | `mermaid-diagrams`, `paperbanana` |
| Full game project with 3D assets | `game-dev` | `blender`, `comfyui` |
| Multi-repo release with CI/CD | `github-release-management` | `github-workflow-automation`, `github-multi-repo` |
| ML model with custom CUDA kernels | `pytorch-ml` | `cuda` |
| Ontology pipeline (create + enrich) | `ontology-core` | `ontology-enrich` |
| PRD to working implementation | `prd2build` | `build-with-quality` |
| Documentation with live web research | `report-builder` | `perplexity-research`, `gemini-url-context` |
| Visual UI testing after building components | `daisyui` or `ui-ux-pro-max-skill` | `playwright` |
| Academic paper with AI-generated figures | `latex-documents` | `paperbanana`, `mermaid-diagrams` |
| Debug web app from Docker | `host-webserver-debug` | `chrome-cdp`, `playwright` |

---

## MCP Server Summary

12 skills provide MCP servers (registered in `skills/mcp.json` or invocable via skill config):

| Skill | Protocol | Entry Point |
|-------|----------|-------------|
| `lazy-fetch` | stdio | `mcp-server/dist/mcp-server.js` |
| `cuda` | stdio | `mcp-server/server.py` |
| `deepseek-reasoning` | mcp-sdk | `mcp-server/server.js` |
| `flow-nexus-neural` | flow-nexus | via `npx flow-nexus@latest` |
| `gemini-url-context` | fastmcp | `mcp-server/server.py` |
| `host-webserver-debug` | mcp-sdk | `mcp-server/server.js` |
| `imagemagick` | fastmcp | `mcp-server/server.py` |
| `openai-codex` | stdio | `mcp-server/server.js` |
| `playwright` | mcp-sdk | `mcp-server/server.js` |
| `qgis` | fastmcp | `mcp-server/server.py` |
| `web-summary` | fastmcp | `mcp-server/server.py` |
| `perplexity` | mcp-sdk | `mcp-server/server.js` (DEPRECATED -- use perplexity-research) |

Additionally, `blender` and `comfyui` have MCP server entries in `mcp.json` but declare `mcp_server` differently in their SKILL.md.

---

## Overlap Analysis and Recommendations

### Overlapping Skill Groups

**1. Perplexity**: `perplexity` is deprecated; `perplexity-research` is the active replacement. Complete.

**2. Swarm Orchestration**: `swarm-orchestration` is deprecated; `swarm-advanced` is the active replacement. Complete.

**3. AgentDB Optimization**: `agentdb-optimization` is deprecated; merged into `agentdb-vector-search`. Complete.

**4. ReasoningBank + Pair Programming**: Both `reasoningbank-agentdb`, `reasoningbank-intelligence`, and `pair-programming` are deprecated; all merged into `build-with-quality`. Complete.

**5. Browser Tools (4 active + 1 meta)**: Well-differentiated. `browser-automation` serves as the meta-skill decision layer. No merge needed.

**6. V3 Implementation Skills (9 archived)**: All archived. Serve as reference only. Could be moved to a `/skills/archive/` subdirectory to reduce clutter.

**7. Development Meta-Skills**: `build-with-quality` vs `sparc-methodology` -- distinct. BWQ focuses on quality engineering with 111+ agents; SPARC focuses on phased methodology. Both are needed.

**8. Flow Nexus Trio**: `flow-nexus-platform`, `flow-nexus-neural`, `flow-nexus-swarm` -- well-separated by concern (platform admin vs ML training vs swarm deployment). No merge needed.

### Potential Future Consolidation

- **AgentDB family** (4 skills): Consider a meta-skill `agentdb` that routes to the correct sub-skill, similar to how `browser-automation` routes browser tools.
- **GitHub family** (5 skills): Consider a meta-skill `github` that routes based on the operation type.
