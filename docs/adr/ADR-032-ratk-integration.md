# ADR-032: RATK Integration for WebXR Session Feature Handling

## Status

Proposed

## Context

`quest3AutoDetector.ts` manually constructs `XRSessionInit` objects with raw
`optionalFeatures` string arrays:

```typescript
const sessionInit: XRSessionInit = {
  optionalFeatures: [
    'local-floor', 'bounded-floor', 'hand-tracking',
    'plane-detection', 'mesh-detection', 'anchors',
    'hit-test', 'dom-overlay'
  ]
};
```

Once the session starts, detected planes, meshes, and anchors are accessed through
untyped `XRFrame` queries. There is no lifecycle management for these spatial
entities -- no add/remove callbacks, no typed wrappers, and no integration with
the Three.js scene graph. The LOD system in `useVRConnectionsLOD.ts` operates on
Three.js objects and cannot reason about spatial anchors because they are not
represented as `THREE.Object3D` instances.

Meta's Reality Accelerator Toolkit (RATK) (`ratk` npm package) provides:

- `RealityAccelerator` class that wraps `XRSession` and manages spatial features.
- `RATKPlane`, `RATKMesh`, `RATKAnchor` classes extending `THREE.Object3D`.
- Lifecycle callbacks: `onPlaneAdded`, `onPlaneRemoved`, `onMeshAdded`,
  `onMeshRemoved`.
- `createAnchor(position, quaternion)` returning a typed `RATKAnchor`.
- `HitTestTarget` for ray-based spatial queries.

The current stack is Three.js 0.183.0 with @react-three/xr 6.6.29.

## Decision Drivers

- **Type safety**: Current code uses `any` casts for XR frame plane/mesh results.
- **Scene graph integration**: LOD system needs spatial entities as Object3D.
- **Maintenance burden**: Manual session feature handling duplicates what RATK does.
- **Bundle size**: XR is loaded lazily; additional KB matters for Quest 3 bandwidth.
- **Compatibility**: Must work with @react-three/xr 6.6.29 which manages its own
  XR session lifecycle.

## Considered Options

### Option 1: Integrate RATK as primary spatial feature manager

Import `ratk` and instantiate `RealityAccelerator` after @react-three/xr creates
the XR session. RATK manages planes, meshes, and anchors; quest3AutoDetector
delegates feature handling to RATK.

**Pros**:
- Typed Plane/Mesh/Anchor objects extending Object3D.
- Lifecycle callbacks reduce manual XRFrame polling.
- Maintained by Meta; aligned with Quest 3 target platform.
- `createAnchor()` API simplifies content placement.
- Approximately 12 KB minified (acceptable for lazy-loaded XR bundle).

**Cons**:
- @react-three/xr 6.6.29 also intercepts session features; dual management risks
  conflicts (e.g. both RATK and R3F/XR listening for plane events).
- RATK expects direct `XRSession` access; R3F/XR abstracts this behind hooks.
- Adds a dependency on Meta-maintained code outside the React Three ecosystem.
- RATK's `update()` must be called each frame; must hook into R3F's `useFrame`.

### Option 2: Build thin typed wrappers without RATK

Create internal `XRPlane`, `XRMeshEntity`, `XRAnchorEntity` classes extending
`THREE.Object3D`. Implement add/remove tracking by diffing `XRFrame` results
each frame.

**Pros**:
- No new dependency.
- Full control over lifecycle and compatibility with R3F/XR.
- Can tailor to VisionClaw's specific needs (e.g. LOD integration).

**Cons**:
- Duplicates RATK's functionality (~400 lines to implement correctly).
- Must handle edge cases RATK already solved (anchor persistence, mesh updates).
- Maintenance burden stays internal.

### Option 3: Use @react-three/xr's built-in spatial features only

Rely on R3F/XR hooks (`usePlanes`, `useAnchors` if available in 6.6.29) without
additional abstraction.

**Pros**:
- Zero additional dependencies.
- Stays within the R3F ecosystem.

**Cons**:
- @react-three/xr 6.6.29 has limited spatial feature support; `usePlanes` and
  `useAnchors` are not stable APIs in this version.
- No Object3D wrappers -- still raw XR API types.
- Does not solve the typing or LOD integration problems.

## Decision

**Option 2: Build thin typed wrappers without RATK.**

Rationale: The compatibility risk between RATK and @react-three/xr 6.6.29 is the
decisive factor. Both libraries assume ownership of the XR session's spatial
feature lifecycle. Resolving conflicts between them would require patching either
RATK or R3F/XR internals, creating fragile coupling to specific versions of both.

Building internal wrappers (~400 lines) is lower risk and allows direct integration
with the LOD system. The wrappers can follow RATK's API surface so that migration
to RATK is straightforward if @react-three/xr adds first-class RATK support in a
future version.

## Consequences

### Positive

- No new runtime dependency for the XR bundle.
- Full compatibility with @react-three/xr 6.6.29 session management.
- Typed spatial entities integrate directly with `useVRConnectionsLOD.ts`.
- Migration path to RATK remains open via API-compatible wrappers.

### Negative

- ~400 lines of internal code to write and test.
- Must track upstream RATK API changes manually if migration is planned.
- Anchor persistence edge cases must be solved independently.

### Neutral

- Bundle size unchanged.
- Quest 3-specific optimisations (e.g. mesh LOD for passthrough) can be added
  incrementally to internal wrappers.

## Links

- [RATK GitHub](https://github.com/meta-quest/reality-accelerator-toolkit)
- [RATK npm](https://www.npmjs.com/package/ratk)
- [@react-three/xr](https://github.com/pmndrs/xr)
- PRD: `docs/prd-xr-modernization.md`
- Related: ADR-033 (Vircadia decoupling)
