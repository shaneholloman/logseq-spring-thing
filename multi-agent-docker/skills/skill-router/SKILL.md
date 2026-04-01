---
name: skill-router
description: >
  Unified dispatcher for 81+ skills. Describe your task; get routed to the optimal skill,
  agent composition, or MCP tool. Single entry point replacing manual decision tree navigation.
  Inspired by the /studio dispatcher pattern from architecture-studio.
version: 1.0.0
author: turbo-flow-claude
tags:
  - routing
  - discovery
  - dispatcher
  - meta-skill
user-invocable: true
---

# /route — Unified Skill Dispatcher

Describe your task. Get routed to the right skill. You don't need to know 81 skills — just say what you need.

## Usage

```
/route [describe what you need]
```

Examples:
- `/route fix the login bug and add tests`
- `/route generate a Wardley map of our infrastructure`
- `/route audit this UI for accessibility`
- `/route research competitor pricing for UK market`
- `/route harden this Linux server for SOC2`
- `/route 123 Main St, Brooklyn — site analysis and zoning`
- `/route make a podcast about our architecture`

## On Start

1. Read the user's input — everything after `/route`.
2. Classify intent against the routing table below.
3. If clear match → invoke that skill immediately.
4. If ambiguous → ask exactly ONE clarifying question, then route.
5. If multi-skill → state the composition plan, then invoke the first skill.

## Routing Table

### Code Development
| If the request involves... | Route to |
|---|---|
| Bug fix, feature implementation, refactoring with tests | `build-with-quality` |
| Systematic multi-phase development (spec → deploy) | `sparc-methodology` |
| PRD → SPEC → ADR traceability, sprint planning | `bhil-methodology` |
| Truth scoring, verification, auto-rollback | `verification-quality` |
| Rust systems programming | `rust-development` |
| WASM + JS graphics interop | `wasm-js` |
| CUDA GPU kernel development | `cuda` |
| React/Next.js/TypeScript/Tailwind conventions | `bencium-code-conventions` |
| Generate docs from a PRD (no code) | `prd2build` |

### GitHub Operations
| If the request involves... | Route to |
|---|---|
| PR review with AI agents | `github-code-review` |
| Cut a release, changelog, deploy | `github-release-management` |
| Create/modify GitHub Actions workflows | `github-workflow-automation` |
| Issues, project boards, sprints | `github-project-management` |
| Cross-repo sync, org-wide automation | `github-multi-repo` |

### Multi-Agent / Swarm
| If the request involves... | Route to |
|---|---|
| Parallel agents (mesh, hierarchical, adaptive) | `swarm-advanced` |
| Queen-led consensus, Byzantine fault tolerance | `hive-mind-advanced` |
| Cloud swarm on Flow Nexus | `flow-nexus-swarm` |
| Sequential pipeline (step N → step N+1) | `stream-chain` |
| Hook automation (pre/post task, session) | `hooks-automation` |
| Lock-free multi-agent VCS (Jujutsu) | `agentic-jujutsu` |

### Research, Web, and Content
| If the request involves... | Route to |
|---|---|
| Live web search with citations | `perplexity-research` |
| Analyse/summarise specific URLs | `gemini-url-context` |
| YouTube transcripts, article summaries | `web-summary` |
| NotebookLM: notebooks, podcasts, slides, quizzes | `notebooklm` |
| LinkedIn profiles, jobs, companies, messaging | `linkedin` |
| Reddit browsing, search, user analysis | `reddit` |
| SEO audit, keyword research, schema markup, GEO | `toprank` |

### Documents and Reports
| If the request involves... | Route to |
|---|---|
| Research report, white paper, policy brief | `report-builder` |
| Academic paper, Beamer presentation, LaTeX | `latex-documents` |
| Diagrams (flowchart, ER, sequence, Gantt, mindmap) | `mermaid-diagrams` |
| Publication-quality academic figures | `paperbanana` |
| Isometric network/infra diagrams | `fossflow` |
| Strategic Wardley maps | `wardley-maps` |
| Validate docs against codebase | `docs-alignment` |

### Media, 3D, and Art
| If the request involves... | Route to |
|---|---|
| Video/audio transcode, edit, stream | `ffmpeg-processing` |
| Image format conversion, resize, batch | `imagemagick` |
| AI image/video generation (SD, FLUX) | `comfyui` |
| 3D modelling and rendering | `blender` |
| Game development (Godot/Unity/Unreal) | `game-dev` |
| 3D Gaussian Splatting | `lichtfeld-studio` |
| Blog headers, infographics, editorial art, comics | `art` (Nano Banana 2) |
| Real-world geography → Minecraft worlds | `terracraft` |
| "Make a video", full video production, explainers, trailers, TTS, avatars, podcast-to-video | `open-montage` |

### Browser Automation
| If the request involves... | Route to |
|---|---|
| Unsure which browser tool | `browser-automation` (meta-skill) |
| Desktop Chrome with login state | `claude --chrome` (built-in) |
| Quick scrape/form-fill, headless | `browser` (agent-browser) |
| Full API, screenshots, visual testing | `playwright` |
| Inspect live Chromium tabs | `chrome-cdp` |
| Host web server from Docker | `host-webserver-debug` |

### AI/ML
| If the request involves... | Route to |
|---|---|
| PyTorch deep learning, model training | `pytorch-ml` |
| Cloud neural training (Flow Nexus) | `flow-nexus-neural` |
| Jupyter notebook experiments | `jupyter-notebooks` |
| Reinforcement learning agents | `agentdb-learning` |
| Delegate reasoning to DeepSeek | `deepseek-reasoning` |
| Delegate coding to GPT-5.4 (simple MCP bridge) | `openai-codex` |
| "Consult with OpenAI", "talk to codex", "ask GPT-5.4", code review by Codex, adversarial review, rescue when stuck | `codex-companion` (`/codex:review`, `/codex:rescue`) |

### Memory and Learning
| If the request involves... | Route to |
|---|---|
| Session/long-term agent memory | `agentdb-memory-patterns` |
| Semantic vector search, RAG, HNSW | `agentdb-vector-search` |
| Distributed multi-DB sync | `agentdb-advanced` |
| Session context, plan tracking, blueprints | `lazy-fetch` |
| "What RuVector tool helps with X?" | `ruvector-catalog` |

### Infrastructure
| If the request involves... | Route to |
|---|---|
| Flow Nexus platform management | `flow-nexus-platform` |
| Swarm performance profiling | `performance-analysis` |
| Creating new skills | `skill-builder` |

### UI/UX Design
| If the request involves... | Route to |
|---|---|
| General UI/UX (palettes, fonts, styles) | `ui-ux-pro-max-skill` |
| daisyUI / Tailwind components | `daisyui` |
| Enterprise UX (WCAG, regulated, ask-first) | `bencium-controlled-ux-designer` |
| Bold creative UX (anti-AI-slop) | `bencium-innovative-ux-designer` |
| Production frontend (Anthropic-quality) | `bencium-impact-designer` |
| Visual design audit / polish | `design-audit` |
| Typography enforcement | `typography` |
| AI-first relationship interfaces | `relationship-design` |

### Software Architecture
| If the request involves... | Route to |
|---|---|
| First-principles design, genuinely new things | `renaissance-architecture` |
| Domain modelling, systems thinking | `human-architect-mindset` |
| Over-engineering / vanity check | `vanity-engineering-review` |
| Entropy vs negentropy evaluation | `negentropy-lens` |
| Communication style detection | `adaptive-communication` |

### Domain-Specific
| If the request involves... | Route to |
|---|---|
| Geospatial GIS, maps | `qgis` |
| Ontology creation (Logseq → OWL2) | `ontology-core` |
| Ontology validation and enrichment | `ontology-enrich` |
| AEO (AI search citation optimisation) | `bencium-aeo` |
| AEC (architecture, construction, zoning, sustainability) | `architecture-studio` (`/studio [task]`) |

### Security
| If the request involves... | Route to |
|---|---|
| Linux hardening, compliance, forensics | `defense-security` |

## Routing Rules

### Rule 1: Clear match → dispatch immediately
State which skill handles the request in one sentence, then invoke it.

### Rule 2: Ambiguous → ask ONE question
If the intent could go to 2+ skills, ask exactly one clarifying question. Then route.

### Rule 3: Multi-skill composition → state the plan
If the task spans multiple skills, state the sequence and invoke the first one.
Example: "Starting with `perplexity-research` for competitor data, then `report-builder` for the analysis document."

### Rule 4: No match → show condensed menu
```
I don't have a specific skill for that. Here's what I cover:

• Code: /route [bug fix / feature / refactor]
• Research: /route [web search / URL analysis / NotebookLM]
• Design: /route [UI/UX / design audit / typography]
• Docs: /route [report / LaTeX / diagrams / Wardley map]
• Media: /route [image / video / 3D / AI art]
• DevOps: /route [GitHub / CI-CD / release]
• Security: /route [hardening / compliance / audit]
• Architecture: /route [review / first-principles / entropy lens]
• AEC: /route [site planning / zoning / sustainability]

Or browse the full inventory: see SKILL-DIRECTORY.md
```

### Rule 5: Just `/route` with no arguments → show condensed menu

## What This Skill Does NOT Do
- It does not execute tasks. It routes to the skill that does.
- It does not override skill-internal logic.
- It does not ask more than one clarifying question.
