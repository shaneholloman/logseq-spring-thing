# PRD: VisionClaw XR Subsystem Modernization

> **Status: Superseded by [ADR-071](../adr/ADR-071-godot-rust-xr-replacement.md) and [PRD-008](../PRD-008-xr-godot-replacement.md). Archived 2026-05-02.**
>
> This PRD targeted the WebXR / Vircadia / Three.js stack which has now been
> removed from the repository. Replacement substrate is Godot + Rust gdext
> (OpenXR) — see PRD-008 and the [removal plan](../xr-vircadia-removal-plan.md).

**Status**: Superseded — see notice above. Original status: Draft
**Priority**: P1 -- XR subsystem is 4 months stale with 17% test coverage
**Affects**: `ImmersiveApp.tsx`, `platformManager.ts`, `quest3AutoDetector.ts`,
`useVRConnectionsLOD.ts`, XR session lifecycle, Vircadia integration
**Target Platform**: Meta Quest 3 via WebXR
**Stack**: Three.js 0.183.0, @react-three/xr 6.6.29, React Three Fiber

---

## 1. Problem Statement

The XR subsystem has accumulated significant technical debt:

| # | Finding | Impact |
|---|---------|--------|
| 1 | ImmersiveApp.tsx and platformManager.ts unchanged since Dec 2025 | 4 months of upstream Three.js/@react-three/xr fixes not integrated |
| 2 | quest3AutoDetector.ts hardcodes Vircadia ClientCore import and `ws://localhost:3020/world/ws` | Quest 3 auto-start fails silently if Vircadia is not running |
| 3 | 2 of 12 XR files have tests (17% coverage) | Regressions go undetected; no CI gate for XR behaviour |
| 4 | `new-quest3` branch is 9 months stale with unmerged voice/audio work | Voice features stuck; merge conflict surface grows weekly |
| 5 | Manual `XRSessionInit` construction with raw `optionalFeatures` arrays | No typed plane/mesh/anchor handling; RATK could replace this |
| 6 | Hand tracking detection hardcodes `isQuest()` gate | Non-Quest devices with hand tracking (e.g. Apple Vision Pro) are excluded |

These issues compound: the tight Vircadia coupling blocks standalone XR testing,
the low coverage means refactoring is high-risk, and the stale branch guarantees
a painful merge.

---

## 2. Goals

| Goal | Measurable Target |
|------|-------------------|
| Decouple device detection from network dependencies | quest3AutoDetector works with Vircadia offline |
| Raise XR test coverage | >= 60% line coverage across all 12 XR files |
| Adopt capability-based detection | Replace `isQuest()` with WebXR feature queries |
| Evaluate RATK integration | Decision documented in ADR-032; prototype if accepted |
| Merge or close `new-quest3` branch | Branch age < 1 sprint (2 weeks) |
| Update Three.js/@react-three/xr | Track latest patch within 0.183.x / 6.6.x |

---

## 3. Non-Goals

- Dropping Quest 3 as primary target in favour of another HMD.
- Migrating away from Three.js to a native XR framework.
- Implementing full Vircadia world-building features inside VisionClaw.
- Supporting WebXR on mobile browsers (phone AR).
- Rewriting the LOD system in `useVRConnectionsLOD.ts` (separate effort).

---

## 4. User Stories

### Quest 3 User

- As a Quest 3 user, I can launch the XR experience without a Vircadia server
  running locally, so that device detection and session entry work standalone.
- As a Quest 3 user, I get typed spatial anchors and plane detection via RATK
  (if adopted), so that passthrough placement is reliable.
- As a Quest 3 user, hand tracking works because the system queries
  `XRSession.inputSources` capabilities, not a device name string.

### Desktop Developer

- As a developer, I can run the full XR test suite without a Quest 3 connected,
  using WebXR emulator polyfills.
- As a developer, I can import quest3AutoDetector without pulling in Vircadia SDK,
  so unit tests are fast and isolated.
- As a developer, I have typed interfaces for XR session features (planes, meshes,
  anchors) so the compiler catches misuse.

### QE Engineer

- As a QE engineer, CI fails if XR test coverage drops below 60%.
- As a QE engineer, I can run visual regression tests for the immersive scene by
  comparing WebGL snapshots against baselines.
- As a QE engineer, the `new-quest3` branch is either merged or archived, so there
  is a single source of truth for XR code.

---

## 5. Technical Requirements

### 5.1 Vircadia Decoupling (ADR-033)

- Extract Vircadia ClientCore initialization from quest3AutoDetector.ts.
- Introduce an `XRNetworkAdapter` interface with `connect()` / `disconnect()`.
- Provide a `VircadiaAdapter` implementation and a `NullAdapter` default.
- quest3AutoDetector receives the adapter via constructor injection.

### 5.2 Capability-Based Detection

- Replace `isQuest()` boolean with `queryXRCapabilities()` returning a typed
  `XRDeviceCapabilities` object: `{ handTracking: boolean, planeDetection: boolean,
  meshDetection: boolean, anchors: boolean, passthrough: boolean }`.
- platformManager.ts consumes capabilities instead of device identity.

### 5.3 RATK Evaluation (ADR-032)

- Prototype `RealityAccelerator` integration in a feature branch.
- Measure bundle size delta (current XR bundle vs. RATK addition).
- Validate compatibility with @react-three/xr 6.6.29 `XR` component.
- Document typed Plane/RMesh/Anchor benefits vs. current raw feature arrays.

### 5.4 Test Coverage

- Add unit tests for quest3AutoDetector (mock XRSystem, XRSession).
- Add integration tests for ImmersiveApp session lifecycle (enter/exit/error).
- Add contract tests for platformManager capability queries.
- Configure coverage threshold in CI: 60% line, 50% branch for `src/xr/`.

### 5.5 Branch Hygiene

- Rebase `new-quest3` onto main; resolve conflicts.
- Cherry-pick voice/audio work into a new `feat/xr-voice` branch if viable.
- Delete `new-quest3` after merge or explicit archive decision.

---

## 6. Success Criteria

| Criterion | Verification |
|-----------|--------------|
| XR session starts without Vircadia running | Manual test on Quest 3 + automated test with NullAdapter |
| Test coverage >= 60% | CI coverage report on `src/xr/` directory |
| No `isQuest()` calls in detection path | Grep returns zero matches in quest3AutoDetector.ts |
| RATK decision documented | ADR-032 status is Accepted or Rejected |
| `new-quest3` branch resolved | Branch deleted or archived in remote |
| Zero TypeScript `any` in XR session feature types | `tsc --noEmit` passes with strict mode |

---

## 7. Timeline

| Phase | Duration | Deliverables |
|-------|----------|--------------|
| 1. Decoupling + Tests | 2 weeks | ADR-033 implemented, NullAdapter, test harness, coverage >= 40% |
| 2. Capability Detection | 1 week | `queryXRCapabilities()`, platformManager refactor |
| 3. RATK Evaluation | 1 week | ADR-032 resolved, prototype branch if accepted |
| 4. Branch Resolution | 1 week | `new-quest3` merged or archived, `feat/xr-voice` if applicable |
| 5. Coverage Push | 1 week | Coverage >= 60%, CI gate enabled |

Total: 6 weeks
