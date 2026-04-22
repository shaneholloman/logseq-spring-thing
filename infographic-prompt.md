# VisionClaw — Master Infographic Prompt

## Brand Aesthetic (Canonical — from `docs/diagrams/src/aesthetic-prompt.md`)

**Every element of this infographic MUST conform to the standing VisionClaw diagram aesthetic:**

```
Professional dark-mode technical diagram in the VisionClaw brand aesthetic:

STYLE:
- Cinematic sci-fi UI concept art fused with engineering blueprint clarity
- Deep midnight-navy background (#0A1020) with subtle volumetric atmospheric haze
- Crystalline nodes with soft inner luminescence and thin bright borders
- Thin directional energy filaments for connections, with gradient glow
- Minimal chrome, maximum signal — every pixel carries meaning
- NO hand-drawn wobble. NO cartoon shapes. NO watercolour. Crisp geometric precision

THREE-LAYER COLOUR SEMANTICS (strictly enforce):
- Violet #8B5CF6 / #A78BFA glow = Governance, human judgment, policy (top layer)
- Cyan #00D4FF / #5DADE2 glow = Orchestration, agents, reasoning (middle layer)
- Emerald #10B981 / #34D399 glow = Discovery, knowledge, ingestion (bottom layer)
- Amber #F59E0B / #FDE68A glow = Trust/provenance accents (sparingly, for critical hubs)
- Off-white #E8F4FC for primary text

COMPOSITION:
- Preserve the structural layout of the reference diagram precisely
- Add depth via subtle drop shadows and rim lighting on each node
- Use geometric primitives consistent with VisionClaw: hexagonal panels,
  icosahedron accents, capsule-shaped nodes for agents, sphere hubs for
  core reasoners
- Background may include extremely faint grid lattice or particle motes for depth
- No decorative clutter — prioritise legibility of labels and relationships

TYPOGRAPHY:
- Clean sans-serif (Inter/IBM Plex Sans vibe) for labels
- Bold headings in all-caps for section titles
- Small italic annotations for secondary notes
- High-contrast white text on dark panels
- Ensure all text from the reference diagram appears verbatim and is legible

OUTPUT:
- Feel: governed, operational, dimensional — not consumer-glossy, not corporate-flat
```

**Mermaid theme** (from `docs/diagrams/src/visionclaw-theme.json`):
- Background: `#0A1020`
- Primary text: `#E8F4FC`
- Node border: `#00D4FF` (cyan)
- Line colour: `#5DADE2`
- Primary border: `#10B981` (emerald)
- Font: Inter, system-ui, sans-serif at 15px
- Flow curve: basis (smooth bezier)

---

## Purpose

Create a **single, large-format technical infographic** (minimum 4000×6000px, portrait orientation) that communicates the entire VisionClaw platform — its three-layer architecture, its core mechanisms, its data flows, and its key metrics — in one visually cohesive composition. The audience is a technical CTO or enterprise architect evaluating the platform for the first time. The infographic must be self-contained: a reader who has never seen VisionClaw should understand what it is, how it works, and why it is different after studying it for 90 seconds.

---

## Section 1 — Title Hero (top 8% of canvas)

**Layout**: Centred, with the VisionClaw wordmark dominating.

**Elements:**
- **"VisionClaw"** in large display type (48–60pt), bold, all-caps, off-white `#E8F4FC` with a subtle cyan rim-light glow. The "V" and "C" may carry faint crystalline faceting.
- **Tagline** beneath in lighter weight, 16pt: *"The governed knowledge mesh where contributors, agents, and ontology compound."*
- **Three badge-style callouts** in a horizontal row beneath the tagline, each in a small hexagonal panel with the appropriate layer glow:
  - `19 DDD Bounded Contexts` (emerald glow — substrate)
  - `92 CUDA Kernels · 55× GPU` (cyan glow — orchestration)
  - `16 MCP Tools · 83 Agent Skills` (violet glow — governance)
- **Background**: Deep midnight navy `#0A1020` with an extremely faint 3D graph network texture — nodes as tiny crystalline dots, edges as hair-thin cyan filaments — fading into the dark. A few particle motes drift in the haze for depth. No decorative clutter.

---

## Section 2 — The Three Strata (central 45% of canvas)

This is the core of the infographic. Three horizontal bands, separated by thin glowing divider lines (each a gradient matching its layer colour), each containing a distinct visual composition.

### Bottom Band — SUBSTRATE (label: "SUBSTRATE — stores, computes, reasons, renders")

**Colour accent**: Emerald `#10B981` glow for discovery/knowledge elements, cyan `#00D4FF` for agent/reasoning elements.

**Header**: "SUBSTRATE" in bold all-caps, emerald glow, with a thin emerald horizontal rule beneath.

**Visual elements** (arranged as crystalline nodes in a loose horizontal row, connected by thin energy filaments):

1. **Graph Data (BC2)** — A sphere hub node (emerald glow, inner luminescence) labelled "Neo4j 5.13". Connected to a mini node-edge graph (5–7 tiny crystalline dots with hair-thin edges). Small italic annotation: "Knowledge Graph · Cypher · bolt protocol"

2. **GPU Physics (BC3)** — A hexagonal panel node (cyan glow) with a GPU chip icon inside. From it, five thin directional energy filaments radiate outward, each labelled in small italic text:
   - `subClassOf → Attraction` (blue-green filament pulling two dots together)
   - `disjointWith → Repulsion` (red-tinged filament pushing dots apart)
   - `physicality → Band clustering` (purple bands)
   - `maturity → Z-axis depth` (vertical green filament)
   - `bridging → Migration pull` (amber filament, glowing brighter)
   Small italic annotation: "92 CUDA kernels · 55× vs CPU · 5 semantic forces · 60 Hz settling"

3. **Ontology & Reasoning (BC7)** — An icosahedron node (violet glow) connected to a "Whelk-rs" capsule node. Small italic annotation: "OWL 2 EL · EL++ subsumption · consistency checking · 90× LRU cache"

4. **Solid Pods** — Three capsule-shaped nodes in a row, each with a faint user silhouette inside and a lock icon. One pod glows amber (trust/provenance accent). Small italic annotation: "Per-user sovereignty · WAC access control · JSON-LD · NIP-98 auth"

5. **Binary Protocol (BC10)** — A thin horizontal stream of binary digits (0s and 1s) flowing rightward, rendered as tiny luminous cyan dots. Small italic annotation: "V5 binary · 9-byte header + 36 bytes/node · 80% bandwidth reduction vs JSON"

6. **Agent Runtime (BC8)** — A small mesh of capsule-shaped agent nodes (cyan glow) connected in a RAFT consensus pattern. Small italic annotation: "Claude-Flow · RAFT hive-mind · 83 core skill modules"

**Connecting filaments**: Thin energy filaments flow upward from Graph Data and Ontology into the Contributor Stratum above. GPU Physics connects to Binary Protocol which feeds WebSocket. All filaments carry a subtle gradient glow matching their source layer colour.

### Middle Band — CONTRIBUTOR STRATUM (label: "CONTRIBUTOR AI SUPPORT STRATUM — assembles, guides, packages, shares")

**Colour accent**: Cyan `#00D4FF` glow for orchestration, emerald `#10B981` for knowledge elements.

**Header**: "CONTRIBUTOR AI SUPPORT STRATUM" in bold all-caps, cyan glow, with a thin cyan horizontal rule beneath.

**Visual elements:**

1. **Contributor Studio (BC18)** — A hexagonal panel containing a mock-up of the four-pane workspace:
   - Left pane: "Ontology Guide Rail" — a vertical column of small crystalline term badges (emerald glow)
   - Centre pane: "Work Lane" — a document/workspace area with faint text lines
   - Right pane: "AI Partner Lane" — a chat-like interaction area with capsule-shaped agent nodes
   - Bottom bar: "Session Memory Bar" — a thin horizontal timeline strip with amber milestone dots
   Small italic annotation: "/studio · Sovereign workspace · NIP-07 → Solid Pod → MCP transparent"
   Label badge: "BC18 · CORE"

2. **Mesh Dojo (BC19)** — A crystalline card node showing a skill card: title "SKILL.md", version badge "v2.1" (emerald glow), eval score "94%" (amber accent). Below the card, a 7-state lifecycle rendered as connected capsule nodes on a horizontal track:
   `Draft → Personal → TeamShared → Benchmarked → MeshCandidate → Promoted → Retired`
   Each state is a tiny capsule; "Promoted" glows violet (governance), "Retired" is dimmed.
   Small italic annotation: "Pod-backed · publicTypeIndex.jsonld · Anthropic v2 discipline"
   Label badge: "BC19 · SUPPORTING"

3. **Ontology Sensei** — A small sphere hub node (cyan glow) with three radial suggestion filaments, each ending in a tiny label bubble:
   - "canonical term" (emerald)
   - "precedent ref" (violet)
   - "skill ref" (cyan)
   Small italic annotation: "3-skill nudges · scoped to focus · ≤1 per 20s · mute per context"

4. **Pod-Native Automations** — A clock/cron icon node (cyan glow) connected by a thin filament to an inbox icon node. Small italic annotation: "NIP-26 scoped delegation caps · /private/automations/ → /inbox/{agent-ns}/"

5. **Share Funnel** — A horizontal funnel shape widening left-to-right, three stages:
   - `Private` (narrow end, lock icon, emerald glow) → `Team` (medium, group icon, cyan glow) → `Mesh` (wide end, globe icon, violet glow)
   Each transition point has a tiny shield icon (Policy Engine gate). The Mesh exit has an amber trust accent.
   Small italic annotation: "Monotonic · Policy Engine gate on every transition · Broker review for Mesh · Append-only audit in /private/contributor-profile/share-log.jsonld"

**Connecting filaments**: The Share Funnel's Mesh exit sends a filament upward into the Management Mesh's Broker node. The Skill Dojo connects to the Broker and Workflow nodes. Sensei connects downward to Ontology in the substrate. All filaments carry directional arrowheads and gradient glow.

### Top Band — MANAGEMENT MESH (label: "MANAGEMENT MESH — governs, measures, adjudicates")

**Colour accent**: Violet `#8B5CF6` glow for governance/judgment, amber `#F59E0B` for trust/provenance accents.

**Header**: "MANAGEMENT MESH" in bold all-caps, violet glow, with a thin violet horizontal rule beneath.

**Visual elements:**

1. **Judgment Broker (BC11)** — A hexagonal panel (violet glow, amber trust accent on the border) containing a stylised broker inbox: a vertical column of 3–4 case cards with priority badges (Critical = red-tinged, High = amber, Medium = cyan, Low = dimmed). One card is "expanded" showing a split-pane Decision Canvas: note content on left (emerald-tinted panel), proposed OWL class on right (violet-tinted panel), diff strip below. A broker avatar silhouette with an amber checkmark.
   Small italic annotation: "BrokerCase · Decision Canvas · 6 outcomes (Approve/Reject/Amend/Delegate/Promote/Precedent) · Nostr-signed provenance bead"
   Label badge: "BC11 · CORE"

2. **Workflow Lifecycle (BC12)** — A horizontal lifecycle track of connected capsule nodes: `Proposal → Review → Pilot → Production → Retirement`. A curved "SUPERSEDES" edge loops from Production back to Proposal. Small italic annotation: "WorkflowProposal · Append-only versions via SUPERSEDES edges · Rollback by pointer swap"

3. **Insight Discovery (BC13)** — An octagonal radar chart (emerald glow) with 8 labelled axes: Wikilink, Co-occurrence, OWL declaration, Agent proposal, Maturity, PageRank, Recency, Authority. A sigmoid curve overlay (thin amber filament) with the 0.60 threshold marked as a bright amber horizontal line. Small italic annotation: "8-signal sigmoid · σ(12·(raw − 0.42)) · 0.60 surface threshold · Monotonic confidence while active"

4. **KPI Observability (BC15)** — A small dashboard grid of 12 metric cards in 3 rows × 4 columns:
   - Row 1 (Mesh KPIs): Surface Precision · Broker Clearance Rate · Ontology Stability · Migration Velocity
   - Row 2 (Mesh KPIs): Provenance Completeness · Rollback Rate · — · —
   - Row 3 (Contributor KPIs): Activation Rate · TTFR · Skill Reuse · Share-to-Mesh
   Each card is a tiny hexagonal panel with a numeric value placeholder and a sparkline.
   Small italic annotation: "12 KPIs · DERIVED_FROM lineage · Event-sourced · Exportable audit reports"

5. **Policy Engine (BC17)** — A shield node (violet glow) with a gear icon inside. Small italic annotation: "6 built-in + 8 contributor-stratum rules · Sub-millisecond evaluation · TOML configuration"

6. **Enterprise Identity (BC14)** — A dual-path node: left path shows an enterprise building icon (OIDC/SAML, violet glow), right path shows a key icon (NIP-98, cyan glow). Both converge to a single user silhouette with an amber trust accent. Small italic annotation: "Dual-stack · OIDC/SAML + NIP-98 · 5 roles (Broker/Admin/Auditor/Contributor/Power Contributor) · Ephemeral keypair delegation"

7. **Connector Ingestion (BC16)** — A GitHub icon node (emerald glow) with a directional filament arrow into the mesh. Small italic annotation: "GitHub-first · Redaction pipeline · Legal review mode · Tier 1/Tier 2 connector framework"

---

## Section 3 — The Migration Event (central hero illustration, spanning all three strata)

**Position**: Centre of the canvas, overlapping the strata divider lines. This is the visual centrepiece and must draw the eye immediately.

**Colour treatment**: The migration event path is rendered as a bright amber `#F59E0B` energy filament (trust/provenance accent) threading through the three layers, with nodes at each stage glowing progressively brighter as the note advances toward ontology.

**Visual narrative** (read left-to-right, with the path curving gently through the three layers):

1. **A Logseq note** (bottom, substrate layer) — A small crystalline card node labelled "Smart Contract.md" with `public:: true` and three wikilinks rendered as tiny emerald filaments connecting to existing OntologyClass nodes. A small amber badge shows "confidence: 0.68". The card has soft inner luminescence.

2. **Detection** — The note card emits a bright amber filament that travels upward. Along the filament, 8 tiny signal icons light up sequentially (wikilink icon, brain icon for agent proposal, checkmark for maturity, etc.), each a tiny luminous dot.

3. **Broker inbox** (top, management mesh layer) — The amber filament arrives at a broker case card in the Judgment Broker panel. The card shows the split-pane Decision Canvas: note on left (emerald-tinted), proposed OWL class on right (violet-tinted), with a bright amber "Approve" action button.

4. **GitHub PR** — From the approve action, the amber filament leads to a GitHub pull-request icon (a small crystalline node with a merge icon). A green Whelk consistency badge (emerald glow) sits beside it. The PR shows "Merge" status.

5. **Bridge edge flip** — From the merge, the filament leads back down to the substrate where a `BRIDGE_TO` edge between two nodes changes from dashed amber (`candidate`) to solid violet (`promoted`). The KGNode accelerates toward its OntologyClass twin.

6. **Physics re-settle** — In the GPU physics layer, the promoted node moves from the peripheral draft zone toward the dense authoritative core. The node pulses with increasing emerald glow as it settles. A small toast notification floats nearby: "Promoted: vc:bc/smart-contract — centrality +0.04"

7. **Nostr bead** — A final small Nostr event icon (capsule shape, amber glow) closes the chain, showing `kind: 30001` with a Whelk consistency hash. A thin amber filament connects back to the original note, completing the provenance loop.

**Timeline callout**: A small clock icon with amber glow and the text "Wall-clock: under 5 minutes" positioned near the bottom of the migration path.

**Label** (positioned prominently near the migration path): "THE MIGRATION EVENT — a Logseq note traverses detection, broker review, PR approval, merge, and physics re-settle. The ontology grows from the bottom up, not the top down."

---

## Section 4 — Dual-Loop Diagram (below the Migration Event, ~10% of canvas)

**Layout**: Two parallel horizontal loops, side by side, each rendered as a circular/oval track of connected capsule nodes.

**Left loop — "Insight Ingestion Loop"**:
Track of 5 capsule nodes (emerald glow): `Discover → Codify → Validate → Integrate → Amplify`
Centre label: "Shadow workflows → governed patterns"
Colour: Emerald `#10B981` glow. Thin emerald filaments connect the nodes.

**Right loop — "Insight Migration Loop"**:
Track of 7 capsule nodes (violet glow): `Discover → Score → Surface → Review → Approve → Merge → Settle`
Centre label: "Shadow concepts → governed OWL classes"
Colour: Violet `#8B5CF6` glow. Thin violet filaments connect the nodes.

**Shared label between them** (in off-white `#E8F4FC`, italic): "Same five-beat pattern, different knowledge units. Both feed the Broker. Both produce Nostr-signed provenance."

---

## Section 5 — Dual-Tier Identity Model (~8% of canvas)

**Layout**: Two horizontal tiers connected by `BRIDGE_TO` edges, rendered as a cross-section diagram.

**Top tier** — "OntologyClass" (slow-moving canon):
- A dense cluster of crystalline sphere nodes (violet glow), tightly packed, stable.
- Small italic annotation: "OWL 2 · Whelk-reasoned · Broker-reviewed · Changes via PR only"

**Bottom tier** — "KGNode" (fast-moving notes):
- A looser scatter of crystalline nodes (emerald glow), more spread out, some drifting.
- Small italic annotation: "Logseq pages · public:: true · Exploratory · Fast-changing"

**Bridge edges** — Three `BRIDGE_TO` filaments between tiers:
- `candidate` — dashed amber filament, moderate glow
- `promoted` — solid violet filament, strong glow
- `colocated` — dotted cyan filament, soft glow

**IRI scheme callout** (in a small hexagonal panel, amber accent): `vc:{domain}/{slug}` — one canonical IRI, two Neo4j labels. Promotion is an edge-label flip, never a node rewrite.

**Label**: "Dual-Tier Identity — KGNode and OntologyClass share one IRI scheme. The BRIDGE_TO edge advances candidate → promoted via the Migration Event."

---

## Section 6 — The Compounding Loop (~6% of canvas)

**Layout**: A horizontal flow diagram showing the three share states as three zones of increasing brightness.

**Visual**:
- Left zone (narrow, dim emerald glow): "PRIVATE" — a single contributor silhouette at a desk, a pod icon with a lock. Small text: "Work lives in contributor's Pod. /private/ by default."
- Middle zone (medium, cyan glow): "TEAM" — 3–4 contributor silhouettes connected by filaments, a shared pod icon. Small text: "ShareIntent → Policy Engine check → WAC mutation → Team visibility."
- Right zone (wide, violet glow): "MESH" — a large network of contributor silhouettes, the full graph visible. Small text: "Broker review → Mesh promotion → Institutional asset. Nostr-signed provenance bead closes the chain."

**Directional filament**: A bright amber arrow flows left-to-right through all three zones, with small shield icons at each transition point (Policy Engine gates).

**Label**: "Three share states, strictly monotonic. The only way down is ContributorRevocation or BrokerRevocation. Every transition is a Policy Engine decision with an append-only audit entry."

---

## Section 7 — Technology Stack Strip (~6% of canvas)

**Layout**: A horizontal strip of hexagonal panel nodes, grouped by layer, each containing a technology icon and label.

| Panel | Content | Glow |
|-------|---------|------|
| Backend | Rust 2021 · Actix-web · 427+ files · Hexagonal CQRS · 9 ports · 12 adapters | Emerald |
| Frontend | React 19 · Three.js · R3F · TypeScript 5.9 · Vite 6 · Radix v3 | Cyan |
| GPU | CUDA 13.1 · cudarc · 92 kernels · 6,585 LOC · PTX ISA auto-downgrade | Cyan |
| Ontology | OWL 2 EL · Whelk-rs · EL++ subsumption · Horned-OWL parser | Violet |
| Identity | NIP-07/98/26 · OIDC/SAML · Solid Pods · WAC · Ephemeral keypairs | Violet |
| Data | Neo4j 5.13 · RuVector · pgvector · HNSW · 1.17M entries · 61µs p50 | Emerald |
| Agents | Claude-Flow · MCP · RAFT · 16 tools · 83 core skills + Dojo-published | Cyan |
| XR | Babylon.js · WebXR · Quest 3 · Vircadia · HRTF spatial · WebGPU/TSL | Emerald |

---

## Section 8 — Platform Metrics Dashboard (~6% of canvas)

**Layout**: A grid of large metric callouts, each in a small hexagonal panel with appropriate layer glow.

| Metric | Value | Context | Glow |
|--------|-------|---------|------|
| GPU speedup | **55×** | vs single-threaded CPU | Cyan |
| CUDA kernels | **92** | 6,585 LOC across 11 files | Cyan |
| Semantic search | **61 µs p50** | RuVector pgvector, 1.17M entries, HNSW 384-dim | Emerald |
| WebSocket latency | **10 ms** | Local network, V5 binary protocol | Emerald |
| Bandwidth reduction | **80%** | Binary V5 vs JSON | Emerald |
| Concurrent XR users | **250+** | Related immersive event | Emerald |
| Physics convergence | **~600 frames** | ~10 seconds, typical graph at rest | Cyan |
| DDD bounded contexts | **19** | Substrate (BC1–BC10) · Mesh (BC11–BC17) · Stratum (BC18–BC19) | Violet |
| MCP tools | **16** | 7 ontology + 9 Contributor Studio | Cyan |
| Agent skills | **83+** | Core + unbounded pod-published via Mesh Dojo | Cyan |
| Skill lifecycle states | **7** | Draft → Personal → TeamShared → Benchmarked → MeshCandidate → Promoted → Retired | Violet |
| Share states | **3** | Private → Team → Mesh (monotonic, Policy Engine gated) | Violet |
| Policy rules | **14** | 6 built-in + 8 contributor-stratum (BC17) | Violet |
| CQRS handlers | **114+** | Commands + Queries across all 19 contexts | Emerald |
| Actix actors | **21+** | Supervised actor tree with GPU supervisor hierarchy | Cyan |

---

## Section 9 — Enterprise Roadmap Timeline (~5% of canvas)

**Layout**: A horizontal timeline with 6 phase milestone nodes, rendered as crystalline hexagons on a thin horizontal filament.

| Phase | Node Label | Description | Weeks | Glow |
|-------|-----------|-------------|-------|------|
| 0 | Platform Coherence | Node types, binary protocol, position flow, settings, ontology edges | 1–6 | Emerald |
| 1 | Identity + Broker MVP | OIDC, roles, Broker Inbox, Decision Canvas | 7–14 | Violet |
| 2 | Insight Ingestion + Migration | WorkflowProposal + MigrationCandidate lifecycles, GitHub connector | 15–24 | Emerald |
| 3 | KPI + Governance | Six mesh KPIs, policy engine, exportable audit reports | 25–32 | Violet |
| 4 | Pilot | Consultancy pilot, connector hardening, success reporting | 33–44 | Amber |
| 5 | Contributor Stratum | /studio MVP, Skill Dojo, share-to-Mesh funnel, automations, Sensei | 45–56 | Cyan |

**Current position**: Phase 5 (sprint 2026-04-20) is highlighted with a bright amber glow and a small "CURRENT" badge. A thin amber pulse ring radiates outward from it.

**Key ADR callouts** beneath the timeline (small italic text):
- Phase 1: ADR-040 · ADR-041
- Phase 2: ADR-042 · ADR-048 · ADR-049
- Phase 3: ADR-043 · ADR-045
- Phase 5: ADR-057 · PRD-003

---

## Section 10 — Footer (~4% of canvas)

**Layout**: Centred, minimal.

**Elements:**
- "VisionClaw is built by DreamLab AI Consulting" in 14pt off-white
- URL: dreamlab-ai.com in cyan
- License badge: "MPL 2.0" in a small hexagonal panel
- Summary line in small italic: "19 Bounded Contexts · 114+ CQRS Handlers · 21+ Actix Actors · 92 CUDA Kernels · 16 MCP Tools · 83+ Agent Skills · 3 Share States · 14 Policy Rules"
- Extremely faint grid lattice fading into the `#0A1020` background at the very bottom edge

---

## Composition Notes

1. **Visual hierarchy**: The three strata dominate the canvas (Section 2). The Migration Event hero illustration (Section 3) draws the eye to the centre. Metrics and technology stack (Sections 7–8) are supporting detail at the bottom.

2. **Data flow filaments**: All connections between components are rendered as thin, semi-transparent directional energy filaments with gradient glow. Colour-code by data type:
   - Emerald filaments: graph data, knowledge, ingestion
   - Cyan filaments: agent orchestration, reasoning, binary protocol
   - Violet filaments: governance decisions, policy evaluations, ontology mutations
   - Amber filaments: trust/provenance, the migration event path, critical hubs

3. **Depth cues**: Subtle drop shadows and rim lighting on every node create depth. The substrate feels "deep" and foundational (slightly darker background haze), the stratum feels "active" and mid-level, the mesh feels "overseeing" and elevated (slightly brighter background haze).

4. **White space**: Do not overcrowd. Each bounded context gets its own clear zone within its stratum band. Use the `#0A1020` dark background as natural spacing. The faint grid lattice provides structure without clutter.

5. **Consistency**: Every bounded context label follows the same format: `BC##` in a small rounded badge with the appropriate layer glow. Every metric follows the same format: large number in bold, small unit, one-line context in italic.

6. **The "so what" scan path**: A reader scanning top-to-bottom should see:
   - (1) What VisionClaw is (title + tagline + badges)
   - (2) How it's structured (three strata with their components)
   - (3) What makes it unique (migration event hero + dual-loop)
   - (4) How sharing works (compounding loop + dual-tier identity)
   - (5) What it's built with (technology stack)
   - (6) How well it performs (metrics dashboard)
   - (7) Where it's going (roadmap timeline)

7. **Geometric primitives**: Strictly use VisionClaw's visual vocabulary:
   - **Hexagonal panels** for bounded contexts, dashboards, and grouped components
   - **Icosahedron accents** for ontology/reasoning nodes
   - **Capsule-shaped nodes** for agents, skills, and lifecycle states
   - **Sphere hubs** for core data stores (Neo4j, Pod, Whelk)
   - **Crystalline card nodes** for documents, notes, and cases

---

## Alternative: Multi-Panel Version

If a single canvas is too dense, the infographic can be split into **4 panels** (each 2000×3000px, portrait, same brand aesthetic):

### Panel A — Architecture Overview
- Title hero (Section 1)
- Three strata (Section 2) — full width, all three bands
- Technology stack strip (Section 7)

### Panel B — The Migration Event
- Migration Event hero (Section 3) — expanded to full panel height
- Dual-tier identity model (Section 5)
- Dual-loop diagram (Section 4)

### Panel C — Contributor Studio Deep-Dive
- Studio four-pane mock-up (expanded, showing each pane in detail)
- Skill lifecycle 7-state track (expanded with annotations)
- Share funnel (expanded with Policy Engine gates)
- Sensei + Automations detail
- Compounding loop (Section 6)

### Panel D — Metrics & Roadmap
- Platform metrics dashboard (Section 8) — expanded with sparklines
- KPI dashboard detail (12 KPIs with lineage)
- Enterprise roadmap timeline (Section 9)
- Footer (Section 10)

Each panel must be self-contained but visually consistent (same `#0A1020` background, same colour semantics, same geometric primitives, same typography).

---

## Key Differentiators to Emphasise Visually

These are the things no other tool combines. The infographic should make them feel like a single integrated system, not a feature list:

1. **The Migration Event itself** — notes become ontology classes through broker review. Nobody else does this. The amber filament threading through all three layers is the visual signature.

2. **Physics from metadata** — OWL properties (`subClassOf`, `disjointWith`, `physicality`, `role`, `maturity`) drive real 3D forces via 92 CUDA kernels. Not decoration; semantics made spatial. The GPU node with radiating force filaments communicates this.

3. **Three strata, one compounding loop** — substrate feeds stratum feeds mesh feeds substrate. The loop is the product. The three colour-coded bands with directional filaments between them make this visible.

4. **Sovereign pods + governed sharing** — data lives in user pods; sharing is monotonic (Private → Team → Mesh), audited, broker-reviewed. The share funnel with Policy Engine shield icons at each gate communicates this.

5. **Dual-stack identity** — enterprise OIDC for access, Nostr NIP-98 for provenance. Both simultaneously. The dual-path identity node with both paths converging on a single user silhouette communicates this.

6. **Workspace, not chat** — the Studio is a cockpit with four panes, not a conversation. The Ramp Glass precedent. The hexagonal Studio panel with its four distinct panes communicates this.

7. **83+ agent skills with eval discipline** — not prompts; versioned, evaluated, benchmarked capabilities with a 7-state retirement lifecycle. The Skill Dojo lifecycle track communicates this.

8. **No LLM opinion-as-fact** — agent confidence is one signal of eight. The 8-signal radar chart with the sigmoid threshold communicates this. The broker is always human.

---

## Reference Diagrams

The following existing diagrams (in `docs/diagrams/`) should be used as structural references for layout and labelling. The infographic should honour their content while upgrading to the full brand aesthetic:

| Diagram | File | Infographic section it informs |
|---------|------|-------------------------------|
| Three-Layer Mesh | `01-three-layer-mesh.png` | Section 2 (Three Strata) |
| Insight Ingestion Cycle | `02-insight-ingestion-cycle.png` | Section 4 (Dual Loops) |
| Four-Plane Voice | `03-four-plane-voice.png` | Technology strip (voice) |
| MCP Tools Radial | `04-mcp-tools-radial.png` | Section 2 (Agent Runtime) |
| System Architecture | `05-architecture-hexagonal.png` | Section 2 (overall layout) |
| Migration Event | `06-migration-event.png` | Section 3 (hero) |
| Dual-Tier Identity | `07-dual-tier-identity.png` | Section 5 |
| Scoring Radar | `08-scoring-radar.png` | Section 2 (Insight Discovery) |
| Physics Outcome | `09-physics-outcome.png` | Section 2 (GPU Physics) |
| Prior Art Quadrant | `10-prior-art-quadrant.png` | Differentiation context |
| Dual Ingestion Loops | `11-dual-ingestion-loops.png` | Section 4 |
| Contributor Stratum | `12-contributor-stratum-layering.png` | Section 2 (middle band) |
| Share State Transitions | `13-adr057-share-state-transitions.png` | Section 6 |
| Skill Lifecycle | `14-adr057-skill-lifecycle.png` | Section 2 (Mesh Dojo) |
| DDD Context Map | `15-ddd-context-map.png` | Section 2 (context labels) |
