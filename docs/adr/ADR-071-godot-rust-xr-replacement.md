# ADR-071: Godot 4 + godot-rust + OpenXR Native APK as the XR Client

**Status:** Accepted
**Date:** 2026-05-02
**Deciders:** jjohare, VisionClaw XR/platform team
**Supersedes:** ADR-032 (RATK integration), ADR-033 (Vircadia decoupling)
**Superseded by:** None
**Related:**
- PRD-008 (XR client native replacement — this ADR is the decision instance)
- DDD `ddd-xr-godot-context.md` (bounded-context model for the new XR client)
- `docs/xr-vircadia-removal-plan.md` (mechanical removal of the legacy stack)
- `docs/threat-model-xr-godot.md`
- `docs/qe-strategy-xr-godot.md`
- `docs/system-architecture-xr-godot.md`
- ADR-061 (Binary Protocol — preserved invariants on the wire)
- ADR-058 (MAD → Agentbox migration — analogous "delete the JS substrate, align with Rust" precedent)
- Swarm coordination: `swarm-1777757491161-nl2bbv`, namespace `xr-godot-replacement`

## TL;DR

The XR client is being moved off WebXR entirely. Three.js + R3F + `@react-three/xr` in `client/src/immersive/` and the Vircadia SDK in `client/src/services/vircadia/` are deleted and replaced by a **native Android APK built from a Godot 4.3 project**, using **godot-rust (gdext)** for performance-critical paths and Rust-substrate alignment, and **OpenXR** as the runtime XR API. Multi-user presence moves from Vircadia's JS/PostgreSQL world server to a new Rust actor (`PresenceActor`) reusing the existing 28 B/node binary protocol (ADR-061). The browser entry-point for VR is removed; Quest 3 users side-load the APK (or install via the Quest Store), which is what they were doing in practice anyway.

## Context

The XR client today is a stack of three poorly-aligned layers:

1. **Two-renderer split.** Desktop rendering uses Three.js + React Three Fiber via `client/src/features/graph/`. XR rendering uses a parallel Three.js + R3F + `@react-three/xr` tree under `client/src/immersive/`, with separate cameras, separate scene graphs, and separate input handling. Edge geometry, label rendering, and SAB position consumption are duplicated and drift independently. ADR-032's "RATK vs. R3F dual-management" risk is the surface symptom; the underlying issue is that maintaining two scene graphs over the same data is structurally wrong.

2. **Vircadia silent-fail coupling.** ADR-033 documents `quest3AutoDetector.ts` hard-coding `ws://localhost:3020/world/ws`. The proposed `XRNetworkAdapter` interface fix was correct in isolation but treats the symptom. The deeper problem is that Vircadia is a JavaScript + PostgreSQL world server running parallel to the Rust substrate — the same node-position state the Rust actor mesh already authoritatively owns. Two sources of truth for spatial state, with the JS one having weaker durability guarantees than the Rust one.

3. **JS/Postgres multi-user does not match the Rust substrate.** Presence, voice, and avatar transforms in Vircadia ride a separate transport, separate identity model (Vircadia user IDs vs. our `did:nostr:<hex-pubkey>`), and separate persistence (Postgres world DB vs. our pod-federated graph storage per ADR-066). Federation (BC20) becomes a 3-way translation problem instead of 2-way.

4. **Browser deployment is not a real win for the target audience.** Quest 3 users who use this app at all already install developer mode and side-load builds; the "click-to-enter VR" web flow is theoretical. We pay the WebXR tax (limited OpenXR extension surface, no Quest scene-mesh API, no spatial anchors API, no body-tracking, JS GC pauses in the render loop) for a deployment convenience nobody uses.

5. **4-month dependency drift.** `@react-three/xr` 6.6.29, Three.js 0.183.0, the Vircadia Web SDK, and RATK have shipped 1–3 majors each since the last alignment. Each upgrade is a 1–2 sprint risk because the dual-renderer + Vircadia surface area is large and integration-test coverage is 17%. The current modernization PRD (`prd-xr-modernization.md`) is an incremental fix to a structural problem.

The user directive (2026-05-02) is to stop trying to incrementally fix WebXR. **There will be one XR client**, native, Rust-aligned, deleted from the browser bundle, and presence will ride the existing binary protocol.

## Decision Drivers

- **Native MR feature parity.** Quest OpenXR extensions (`XR_FB_passthrough`, `XR_FB_scene`, `XR_FB_spatial_entity`, `XR_FB_body_tracking`, `XR_FB_face_tracking`, `XR_FB_hand_tracking_aim`) are not exposed through WebXR or have lossy partial bindings. Native OpenXR is the only path to feature parity with the Quest platform.
- **Rust substrate alignment.** Existing crates (`binary-protocol`, `graph-cognition-core`, `crates/visionclaw-uri`) already encode the canonical model. godot-rust (`gdext`) lets us link those crates directly into the XR client binary instead of re-encoding them in TypeScript.
- **Performance ceiling.** No JS GC, no DOM, direct GPU upload paths via Godot's RenderingServer. Frame-time budget on Quest 3 (11.1 ms at 90 Hz) is unforgivingly tight for a JS render loop competing with WebXR compositor overhead.
- **Single source of truth for graph data.** The 28 B/node binary protocol (ADR-061) becomes the only wire for positions, consumed by both desktop (browser) and XR (APK) clients. The `analytics_update` side message likewise. Vircadia's parallel state goes away.
- **Operational simplification.** Removing Vircadia removes a Postgres instance, a WebSocket server (port 3020), a separate identity domain, a separate auth path, and a federation (BC20) translation hop.
- **Tooling maturity.** Godot 4.3 ships a stable scene editor, GLTF import that round-trips with our existing asset pipeline, and an Android export template that produces a signed APK out of the box. The OpenXR plugin is first-party and tracks Khronos releases.

## Considered Options

### Option 1 (chosen): Godot 4 + OpenXR + godot-rust native APK

**Architecture.** Godot 4.3 project at `xr-client/` with scenes, materials, and UI in GDScript and `.tscn` files. Performance-critical and substrate-shared logic — binary-protocol decode, graph state, presence, URN handling — lives in a Rust crate at `crates/visionclaw-xr-client/` exposed to Godot via gdext. OpenXR runtime via Godot's first-party `OpenXRPlugin`. Build target: Android `.apk` for Quest 3 (Adreno 740, Android 12L base).

**Pros.**
- Full OpenXR extension access via Godot's `OpenXRExtensionWrapperExtension` plumbing; passthrough, scene mesh, anchors, body/face/hand tracking all reachable.
- Mature scene editor reduces hand-built scene-graph code (this is where Bevy still hurts).
- godot-rust 0.2+ (`gdext`) is binding-stable, ABI-checked, and lets us link existing Rust workspace crates directly.
- Android export template is proven on Quest 3; signing, manifest, and OpenXR loader integration are well-trodden paths.
- MIT-style licence (Godot is MIT; gdext is MIT).
- Hybrid GDScript-for-UI + Rust-for-substrate keeps binding ceremony off the UI layer where iteration speed matters.

**Cons.**
- Loses "click-to-enter VR" from the browser. Quest users side-load (developer mode) or install via the Quest Store. Acceptable per Decision Drivers.
- godot-rust learning curve for the team (estimate 1 sprint for two developers to be fluent).
- Separate APK build pipeline in CI (Android SDK + Godot headless export). Adds ~6 min to the XR build leg; runs in parallel with the browser build.
- Godot's renderer is forward+ clustered, not the same pipeline as Three.js; some custom shaders need porting (glass edges, instanced labels).

### Option 2: Godot 4 HTML5 export + WebXR

**Pros.** Preserves browser entry-point. One Godot project, two export targets (Android APK + HTML5).

**Cons.** Loses the entire reason for switching. WebXR through Godot's HTML5 export still goes through the browser's WebXR layer; no Quest OpenXR extensions, no scene mesh, no spatial anchors. Bundle size for a Godot HTML5 export is 30–50 MB compressed (engine + project + Mono if used) — slower first load than the current Three.js bundle. We pay all the costs of the Godot migration and keep the WebXR ceiling.

### Option 3: Bevy + bevy_oxr native APK

**Pros.** Purest Rust story; no language boundary at all. Full ECS leverage, deterministic systems ordering, and direct reuse of every workspace crate without binding ceremony. Renderer is wgpu — same backend Naga generation we already understand.

**Cons.** `bevy_oxr` is alpha (0.x), tracking Bevy releases that themselves have breaking changes every minor version. **No scene editor** — every UI panel, every avatar pose, every menu has to be code-built. GLTF tooling is improving but not at parity with Godot's import pipeline (PBR materials, animation retargeting, blend shapes for face tracking). For a project that needs to ship on Quest 3 in a defined timeline, Bevy is the right choice 12 months from now, not today.

### Option 4: Unity + OpenXR

**Pros.** Cross-platform. Best-in-industry tooling. Mature OpenXR + Meta XR SDK integration. Largest pool of XR developers if hiring becomes relevant.

**Cons.** C# is two language boundaries from our Rust substrate (C# → C ABI → Rust). Licence model has the well-documented runtime-fee history; even with the 2024 walk-back, building a long-lived foundation on a closed-source engine with shifting commercial terms is a known risk. Unity runtime adds ~250 MB to the APK. C# GC has improved but is not free, and the Unity render loop is not under our control in the same way Godot's `_process` / `_physics_process` / Rust gdext signals are.

### Option 5: StereoKit

**Pros.** Excellent MR feature set, OpenXR-native, designed for hand-and-passthrough-first interactions. Lightweight runtime. Microsoft-sponsored but fully open source (MIT).

**Cons.** C#/.NET — same Rust-distance issue as Unity. Anchored to the Microsoft ecosystem; Meta-platform support is best-effort. Renderer is geared toward UI-in-space, not large-graph instanced rendering with thousands of nodes and edges; we would be re-implementing GraphManager-equivalents from primitives. Strong fit for a different product, weak fit for ours.

### Option 6: Continue `prd-xr-modernization.md` (incremental WebXR fix)

**Pros.** Smallest immediate engineering cost. Preserves browser entry-point. ADR-032 + ADR-033 are concrete, scoped fixes.

**Cons.** Does not address any of the five structural failures listed in Context. Two-renderer split, Vircadia coupling, JS/Postgres state divergence, dependency-drift cadence, and the WebXR feature ceiling all remain. Each future MR feature request reopens this same decision.

## Decision

**Option 1: Godot 4 + OpenXR + godot-rust native APK.**

Rationale: it is the only option that simultaneously (a) lifts the Quest OpenXR feature ceiling, (b) keeps us in the Rust substrate for substrate-shared concerns, (c) ships within a defined timeline because the editor and Android export template are mature, and (d) deletes more code than it adds (Vircadia, dual renderer, R3F/XR, RATK shim, all gone). Bevy is the right answer in a future where `bevy_oxr` is stable and the editor exists; until then, Godot's hybrid GDScript-for-scenes + gdext-for-substrate is the pragmatic Rust-aligned choice. Unity and StereoKit lose on Rust distance; HTML5 Godot loses on the entire reason for migrating; incremental WebXR loses on structure.

## Implementation Plan

Phased per PRD-008. Concrete deliverables in this repository:

**Phase 0 — Scaffold (1 sprint).**
- New directory `xr-client/` containing the Godot 4.3 project (`project.godot`, `scenes/`, `scripts/`, `addons/`).
- New workspace member `crates/visionclaw-xr-presence/` (Rust) for the presence client library; gdext-exposed.
- New workspace member `crates/visionclaw-xr-client/` (Rust) for the in-APK substrate logic (binary-protocol decode, graph-state mirror, URN handling, settings).
- Server-side: new actor `src/actors/presence_actor.rs` and handler `src/handlers/presence_handler.rs` carrying avatar transforms and voice room membership over the existing WebSocket using a new `presence_update` JSON message and a per-user 64 B avatar-transform binary frame.
- CI: new `xr-apk` build leg using Godot 4.3 headless export to Android, signed with a project-managed debug keystore for internal builds. Release signing deferred to Phase 3.

**Phase 1 — Render parity (2 sprints).**
- Port GraphManager, GlassEdges, InstancedLabels, and CrystalOrb/Gem/AgentCapsule node geometries to Godot RenderingServer instanced calls, driven by the gdext-bridged `BinaryProtocolDecoder` reading WebSocket frames.
- Wire the existing 28 B/node frame consumer; reuse `crates/binary-protocol` directly via gdext.
- OpenXR session bring-up via Godot's `OpenXRInterface`; passthrough on by default for MR.

**Phase 2 — Multi-user presence (2 sprints).**
- `PresenceActor` server-side; per-user transform broadcast at 30 Hz capped (delta-encoded).
- Voice via Quest's native mic + WebRTC peer mesh (room membership through `PresenceActor`); no SFU in v1.
- Spatial anchors via `XR_FB_spatial_entity`; UUIDs stored against the user's pod (ADR-066) so anchors persist across sessions and devices.

**Phase 3 — Cutover and removal (1 sprint).**
- Tag `pre-godot-xr` on `main` immediately before merge of `feat/xr-godot-cutover`.
- Remove `client/src/immersive/`, `client/src/services/vircadia/`, `quest3AutoDetector.ts`, the Vircadia docker compose service, the world-DB Postgres instance, and all references to ADR-032 / ADR-033 from active docs (ADRs themselves stay, marked Superseded).
- Per `docs/xr-vircadia-removal-plan.md`.
- Browser bundle size drops by the WebXR + Vircadia SDK + R3F/XR weight (~600 KB compressed estimated).

## Consequences

### Positive

- Native Quest OpenXR feature surface available: passthrough, scene mesh, spatial anchors, body / face / hand tracking, all first-class.
- Single source of truth for graph state. Binary protocol is the only wire; Vircadia's PostgreSQL world DB and its operational burden are deleted.
- Rust substrate reach extends into the XR client. Workspace crates link directly via gdext; the encoding/decoding/URN logic stops being re-implemented in TypeScript.
- Frame-time headroom on Quest 3. Removing JS/GC and the WebXR compositor overhead opens the path to higher node counts at 90 Hz.
- Identity unification. `did:nostr:<hex-pubkey>` is the only user identifier across desktop and XR; no Vircadia user-ID translation.
- Build pipeline simplification. Federation (BC20) loses one of three translation domains.
- ADR-032 and ADR-033 retired; the dual-management and silent-fail problems are deleted by removing the modules they were patching.

### Negative

- Browser-based "click to enter VR" is gone. Users install the APK via Quest developer mode (existing path) or eventually via the Quest Store (Phase 3+). This is consistent with how the audience already uses the product but is a real reduction in surface area.
- Separate APK build pipeline in CI. Godot headless export + Android SDK + signing add ~6 min to the build, parallelisable with the browser build.
- godot-rust learning curve. Two team members estimated at 1 sprint each to fluency; mitigated by gdext's strong type system and the substantial public example corpus.
- Shader/material porting. Glass edges, instanced labels, and the WASM scene effects (`client/crates/scene-effects/`) need Godot equivalents. Scene-effects WASM stays in the browser bundle for desktop; the XR client gets native shader equivalents written in Godot Shading Language.
- Quest Store submission (if pursued in Phase 3+) carries Meta's review process, which has its own latency and content rules.

### Neutral

- GDScript remains in use for scene wiring, UI panels, and high-level lifecycle. This is a deliberate hybrid: the binding ceremony of pushing UI into Rust is not worth it for this layer. Substrate logic is Rust; presentation glue is GDScript.
- Desktop browser client is unchanged by this ADR. It continues to render via Three.js + R3F, consume the same binary protocol, and read the same `analytics_update` side messages (ADR-061). The only desktop-side delta is the deletion of the unused `client/src/immersive/` tree.
- Godot version is pinned to 4.3.x for the lifetime of Phase 0–3. Major-version upgrade decisions go through a follow-up ADR.

## Migration & Rollback

**Migration.** Mechanical removal sequence in `docs/xr-vircadia-removal-plan.md`. `feat/xr-godot-cutover` is the integration branch; merges to `main` only after Phase 1 render parity is demonstrated against a fixture graph and Phase 2 presence is end-to-end with two Quest 3 headsets.

**Rollback.** The `pre-godot-xr` tag on `main` (cut at Phase 3 step 1) is preserved indefinitely. The `feat/xr-godot-cutover` branch is preserved for **two sprints** post-merge as a hot-revert path; if a structural defect surfaces in that window, `git revert` of the cutover merge restores the WebXR client. After two sprints, the branch is deleted and rollback requires a new feature-branch port forward of any intervening changes onto the pre-Godot tree — at which point the cost of rollback exceeds the cost of fixing forward, and that is the intended operating point.

**Vircadia data.** Vircadia's world DB contains no data we do not already own elsewhere — avatars, transforms, and voice room state are session-ephemeral. Persistent state (anchors) is migrated to per-user pods (ADR-066) before Vircadia decommission. No data migration script is required for the world DB itself; it is dropped.

## Telemetry / observability

New metrics (server side, Prometheus):
- `presence_clients_connected` (gauge).
- `presence_avatar_frame_bytes_total` (counter).
- `presence_voice_room_count` (gauge).

New metrics (client side, exported via Godot to a lightweight ingest endpoint):
- `xr_frame_time_ms_bucket` (histogram, p50/p95/p99 tracked).
- `xr_openxr_extension_active{name}` (gauge, 0/1 per extension).
- `xr_passthrough_active` (gauge).
- `xr_anchor_count` (gauge).

Expected steady-state at 25k-node graph, single user: frame time p95 < 11 ms, presence wire ≈ 1.9 KB/s per remote user (64 B × 30 Hz), no Vircadia traffic.

## References

- PRD-008 (`docs/prd-008-xr-client-native-replacement.md`)
- DDD context: `docs/ddd-xr-godot-context.md`
- Removal plan: `docs/xr-vircadia-removal-plan.md`
- Threat model: `docs/threat-model-xr-godot.md`
- QE strategy: `docs/qe-strategy-xr-godot.md`
- System architecture: `docs/system-architecture-xr-godot.md`
- Superseded: ADR-032 (RATK integration), ADR-033 (Vircadia decoupling)
- Preserved: ADR-061 (Binary Protocol — wire unchanged), ADR-066 (Pod-federated graph storage — anchor persistence target), ADR-058 (MAD → Agentbox migration — analogous "delete the JS substrate" precedent)
- External: [OpenXR 1.1 specification](https://registry.khronos.org/OpenXR/specs/1.1/html/xrspec.html), [godot-rust gdext](https://github.com/godot-rust/gdext), [Godot 4.3 OpenXR docs](https://docs.godotengine.org/en/stable/tutorials/xr/openxr_module.html), [Meta Quest OpenXR extensions](https://developers.meta.com/horizon/documentation/native/android/mobile-openxr-extensions/)
- Swarm coordination: `swarm-1777757491161-nl2bbv`, memory namespace `xr-godot-replacement`
