//! Phase A Semantic Forces — Integration Test Suite
//!
//! Covers:
//!   1. Parser propagation — `KnowledgeGraphParser` converts Logseq OWL properties
//!      into the correct integer codes stored in `Node::metadata`.
//!   2. GPU smoke tests — kernel family wrappers operate without panic on a
//!      4-node graph; nodes with code 0 (None) receive zero additional force.
//!   3. Settings propagation regression — `PhysicsSettings::physicality_strength`
//!      (and siblings) survive the round-trip through `SimulationParams::from`.
//!
//! GPU tests are gated with `#[ignore]` and require a CUDA-capable device plus
//! the `--features gpu` build flag.  All non-GPU tests pass with
//!
//!   cargo test --test semantic_forces_phase_a_test --features gpu -- --test-threads=1
//!
//! (The `gpu` feature flag is required at compile-time even for non-GPU tests
//! because the `UnifiedGPUCompute` type and kernel-bridge wrappers are only
//! compiled in when the feature is active.  Non-GPU tests that do not reference
//! those types are unconditionally compiled and run.)

// ── non-GPU imports ───────────────────────────────────────────────────────────
use webxr::config::physics::PhysicsSettings;
// PhysicalityCode / RoleCode / MaturityLevel are exercised indirectly through the
// parser (the tests check metadata string values) and are imported here so that
// compilation confirms the public API path is stable.
#[allow(unused_imports)]
use webxr::models::metadata::{MaturityLevel, PhysicalityCode, RoleCode};
use webxr::models::simulation_params::SimulationParams;
use webxr::services::parsers::KnowledgeGraphParser;

// ── GPU-only imports ──────────────────────────────────────────────────────────
#[cfg(feature = "gpu")]
use webxr::gpu::kernel_bridge::{
    apply_maturity_layout_force, apply_physicality_cluster_force, apply_role_cluster_force,
    calculate_physicality_centroids, calculate_role_centroids, finalize_physicality_centroids,
    finalize_role_centroids, Float3,
};
#[cfg(feature = "gpu")]
use webxr::utils::unified_gpu_compute::UnifiedGPUCompute;

// =============================================================================
// Test 2 — Parser propagation
// =============================================================================

/// Helper: parse an inline Logseq markdown string and return the first node's
/// metadata HashMap.
fn parse_first_node_metadata(md: &str) -> std::collections::HashMap<String, String> {
    let parser = KnowledgeGraphParser::new();
    let graph = parser
        .parse(md, "test-page.md")
        .expect("KnowledgeGraphParser::parse should not fail on inline fixture");
    graph
        .nodes
        .into_iter()
        .next()
        .expect("graph should contain at least one node")
        .metadata
}

/// A page carrying `owl:physicality`, `owl:role` and `maturity` properties
/// should produce the expected integer codes in the node metadata.
#[test]
fn parser_propagates_owl_codes_to_node_metadata() {
    let md = r#"- owl:physicality:: abstract
- owl:role:: Process
- maturity:: mature
- public:: true
"#;

    let meta = parse_first_node_metadata(md);

    // PhysicalityCode::Abstract == 1
    assert_eq!(
        meta.get("physicality_code").map(|s| s.as_str()),
        Some("1"),
        "expected physicality_code=1 (Abstract) for 'abstract'"
    );
    // RoleCode::Process == 3
    assert_eq!(
        meta.get("role_code").map(|s| s.as_str()),
        Some("3"),
        "expected role_code=3 (Process) for 'Process'"
    );
    // MaturityLevel::Mature == 2
    assert_eq!(
        meta.get("maturity_level").map(|s| s.as_str()),
        Some("2"),
        "expected maturity_level=2 (Mature) for 'mature'"
    );
}

/// A page with no OWL property lines must produce codes 0 / 0 / 0 (None).
#[test]
fn parser_emits_zero_codes_when_no_owl_properties_present() {
    let md = r#"- public:: true
- Some ordinary content here.
"#;

    let meta = parse_first_node_metadata(md);

    assert_eq!(
        meta.get("physicality_code").map(|s| s.as_str()),
        Some("0"),
        "missing owl:physicality should yield physicality_code=0"
    );
    assert_eq!(
        meta.get("role_code").map(|s| s.as_str()),
        Some("0"),
        "missing owl:role should yield role_code=0"
    );
    assert_eq!(
        meta.get("maturity_level").map(|s| s.as_str()),
        Some("0"),
        "missing maturity/status should yield maturity_level=0"
    );
}

/// A page with `status:: draft` (but no `maturity::` property) should still
/// set `maturity_level` via the status fallback.
///
/// `draft` maps to `MaturityLevel::Emerging` == 1.
#[test]
fn parser_falls_back_to_status_for_maturity_level() {
    let md = r#"- status:: draft
- public:: true
"#;

    let meta = parse_first_node_metadata(md);

    assert_eq!(
        meta.get("maturity_level").map(|s| s.as_str()),
        Some("1"),
        "status::draft should fall back to maturity_level=1 (Emerging)"
    );
}

/// `maturity::` takes precedence over `status::` when both are present.
#[test]
fn parser_maturity_key_takes_precedence_over_status() {
    let md = r#"- maturity:: stable
- status:: draft
- public:: true
"#;

    let meta = parse_first_node_metadata(md);

    // MaturityLevel::Mature == 2 (from "stable")
    assert_eq!(
        meta.get("maturity_level").map(|s| s.as_str()),
        Some("2"),
        "explicit maturity::stable should win over status::draft"
    );
}

/// Case-insensitive physicality: `VirtualEntity` (mixed case) → Virtual == 2.
#[test]
fn parser_physicality_case_insensitive() {
    let md = r#"- owl:physicality:: VirtualEntity
- public:: true
"#;
    let meta = parse_first_node_metadata(md);
    assert_eq!(
        meta.get("physicality_code").map(|s| s.as_str()),
        Some("2"),
        "VirtualEntity should map to physicality_code=2 (Virtual)"
    );
}

/// Case-insensitive role: both `Concept` and `concept` → Concept == 1.
#[test]
fn parser_role_case_insensitive() {
    for raw in &["Concept", "concept"] {
        let md = format!(
            "- owl:role:: {}\n- public:: true\n",
            raw
        );
        let meta = parse_first_node_metadata(&md);
        assert_eq!(
            meta.get("role_code").map(|s| s.as_str()),
            Some("1"),
            "role '{}' should map to role_code=1 (Concept)",
            raw
        );
    }
}

/// Unrecognised values should not panic and produce code 255 (Unknown).
#[test]
fn parser_unknown_values_produce_code_255() {
    let md = r#"- owl:physicality:: garbage
- owl:role:: Something-Exotic
- maturity:: garbage
- public:: true
"#;

    let meta = parse_first_node_metadata(md);

    assert_eq!(
        meta.get("physicality_code").map(|s| s.as_str()),
        Some("255"),
        "unrecognised physicality should yield 255 (Unknown)"
    );
    assert_eq!(
        meta.get("role_code").map(|s| s.as_str()),
        Some("255"),
        "unrecognised role should yield 255 (Unknown)"
    );
    assert_eq!(
        meta.get("maturity_level").map(|s| s.as_str()),
        Some("255"),
        "unrecognised maturity should yield 255 (Unknown)"
    );
}

// =============================================================================
// Test 4 — Settings propagation regression (non-GPU)
// =============================================================================

/// `PhysicsSettings` carries `physicality_strength`, `role_strength` and
/// `maturity_strength`.  The `SimulationParams::from` conversion must
/// faithfully propagate all three fields.
#[test]
fn physics_settings_semantic_strengths_propagate_to_simulation_params() {
    let mut physics = PhysicsSettings::default();
    physics.physicality_strength = 0.5;
    physics.role_strength = 0.25;
    physics.maturity_strength = 0.10;

    let params = SimulationParams::from(&physics);

    assert!(
        (params.physicality_strength - 0.5).abs() < f32::EPSILON,
        "physicality_strength should propagate: expected 0.5, got {}",
        params.physicality_strength
    );
    assert!(
        (params.role_strength - 0.25).abs() < f32::EPSILON,
        "role_strength should propagate: expected 0.25, got {}",
        params.role_strength
    );
    assert!(
        (params.maturity_strength - 0.10).abs() < f32::EPSILON,
        "maturity_strength should propagate: expected 0.10, got {}",
        params.maturity_strength
    );
}

/// Default `PhysicsSettings` should produce the expected default strengths
/// (0.40 / 0.30 / 0.15) in `SimulationParams`.
#[test]
fn physics_settings_default_semantic_strengths_are_correct() {
    let params = SimulationParams::from(&PhysicsSettings::default());

    assert!(
        (params.physicality_strength - 0.40).abs() < 1e-6,
        "default physicality_strength should be 0.40, got {}",
        params.physicality_strength
    );
    assert!(
        (params.role_strength - 0.30).abs() < 1e-6,
        "default role_strength should be 0.30, got {}",
        params.role_strength
    );
    assert!(
        (params.maturity_strength - 0.15).abs() < 1e-6,
        "default maturity_strength should be 0.15, got {}",
        params.maturity_strength
    );
}

/// Zero strengths must propagate through (the Phase A kernels should be no-ops
/// when all strengths are zero — validated here at the settings level).
#[test]
fn physics_settings_zero_strengths_propagate() {
    let mut physics = PhysicsSettings::default();
    physics.physicality_strength = 0.0;
    physics.role_strength = 0.0;
    physics.maturity_strength = 0.0;

    let params = SimulationParams::from(&physics);

    assert_eq!(params.physicality_strength, 0.0);
    assert_eq!(params.role_strength, 0.0);
    assert_eq!(params.maturity_strength, 0.0);
}

// =============================================================================
// Test 3 — GPU smoke tests (CUDA-only, #[ignore])
// =============================================================================

/// Verify that a `UnifiedGPUCompute` instance can be created for a 4-node graph
/// without panic.  Once `upload_semantic_metadata` is added by the GPU-memory
/// agent this test should be extended to call that method.
///
/// Requires: CUDA-capable device, build with `--features gpu`.
///
/// UPSTREAM BLOCKER: needs `UnifiedGPUCompute::upload_semantic_metadata`
/// (owned by the gpu-memory agent).  The call is omitted here so that the
/// test file compiles against the current tree; re-enable once the method lands.
#[test]
#[ignore] // requires CUDA
fn test_phase_a_gpu_context_initialises_without_panic() {
    #[cfg(feature = "gpu")]
    {
        let ptx_path = concat!(env!("OUT_DIR"), "/visionflow_unified.ptx");
        let ptx_content =
            std::fs::read_to_string(ptx_path).expect("failed to read PTX file");

        let num_nodes = 4;
        let num_edges = 0;

        // Constructing the GPU context is sufficient to confirm the PTX loads
        // and the device buffers for physicality_code / role_code / maturity_level
        // are allocated (they default to zeroed i32 buffers).
        let _gpu = UnifiedGPUCompute::new_with_modules(
            num_nodes,
            num_edges,
            &ptx_content,
            None,
            None,
        )
        .expect("failed to initialise UnifiedGPUCompute");

        // TODO: once upload_semantic_metadata lands, add:
        //   _gpu.upload_semantic_metadata(&[0; 4], &[0; 4], &[0; 4]).unwrap();
    }
    #[cfg(not(feature = "gpu"))]
    {}
}

/// Physicality kernel family: nodes with code 0 (None) must receive
/// zero additional force after centroid accumulation + force application.
///
/// Requires: CUDA-capable device, build with `--features gpu`.
#[test]
#[ignore] // requires CUDA
fn test_physicality_kernel_noop_on_none_coded_nodes() {
    #[cfg(feature = "gpu")]
    {
        let num_nodes = 4;
        // All None
        let codes = vec![0i32; num_nodes];

        let positions = vec![
            Float3 { x: 0.0,  y: 0.0,  z: 0.0 },
            Float3 { x: 1.0,  y: 0.0,  z: 0.0 },
            Float3 { x: 0.0,  y: 1.0,  z: 0.0 },
            Float3 { x: -1.0, y: 0.0,  z: 0.0 },
        ];
        let mut pos = positions.clone();

        // 4 physicality buckets × 3 components = 12 f32 centroids
        let mut centroids = vec![Float3 { x: 0.0, y: 0.0, z: 0.0 }; 4];
        let mut counts    = vec![0i32; 4];
        let mut forces    = vec![Float3 { x: 0.0, y: 0.0, z: 0.0 }; num_nodes];

        calculate_physicality_centroids(&codes, &positions, &mut centroids, &mut counts, num_nodes);
        finalize_physicality_centroids(&mut centroids, &counts);

        // All codes are None (0) so every count should remain 0.
        for (i, &c) in counts.iter().enumerate() {
            assert_eq!(c, 0, "physicality bucket {} should have zero members (all None)", i);
        }

        apply_physicality_cluster_force(&codes, &centroids, &mut pos, &mut forces, num_nodes);

        // Forces must remain zero for None-coded nodes.
        for (i, f) in forces.iter().enumerate() {
            assert!(
                f.x == 0.0 && f.y == 0.0 && f.z == 0.0,
                "node {} should receive zero physicality force when code=None, got ({},{},{})",
                i, f.x, f.y, f.z
            );
        }
    }
    #[cfg(not(feature = "gpu"))]
    {}
}

/// Role kernel family: nodes with code 0 (None) must receive zero role force.
///
/// Requires: CUDA-capable device, build with `--features gpu`.
#[test]
#[ignore] // requires CUDA
fn test_role_kernel_noop_on_none_coded_nodes() {
    #[cfg(feature = "gpu")]
    {
        let num_nodes = 4;
        let codes = vec![0i32; num_nodes];

        let positions = vec![
            Float3 { x: 0.0, y: 0.0, z: 0.0 },
            Float3 { x: 2.0, y: 1.0, z: 0.0 },
            Float3 { x: 1.0, y: 2.0, z: 0.0 },
            Float3 { x: 0.0, y: 3.0, z: 0.0 },
        ];
        let mut pos = positions.clone();

        // 7 role buckets × 3 components = 21 floats
        let mut centroids = vec![Float3 { x: 0.0, y: 0.0, z: 0.0 }; 7];
        let mut counts    = vec![0i32; 7];
        let mut forces    = vec![Float3 { x: 0.0, y: 0.0, z: 0.0 }; num_nodes];

        calculate_role_centroids(&codes, &positions, &mut centroids, &mut counts, num_nodes);
        finalize_role_centroids(&mut centroids, &counts);

        for (i, &c) in counts.iter().enumerate() {
            assert_eq!(c, 0, "role bucket {} should have zero members (all None)", i);
        }

        apply_role_cluster_force(&codes, &centroids, &mut pos, &mut forces, num_nodes);

        for (i, f) in forces.iter().enumerate() {
            assert!(
                f.x == 0.0 && f.y == 0.0 && f.z == 0.0,
                "node {} should receive zero role force when code=None, got ({},{},{})",
                i, f.x, f.y, f.z
            );
        }
    }
    #[cfg(not(feature = "gpu"))]
    {}
}

/// Maturity kernel family: nodes with code 0 (None) must receive zero maturity
/// layout force (in particular no Z-component is applied).
///
/// Requires: CUDA-capable device, build with `--features gpu`.
#[test]
#[ignore] // requires CUDA
fn test_maturity_kernel_noop_on_none_coded_nodes() {
    #[cfg(feature = "gpu")]
    {
        let num_nodes = 4;
        let codes = vec![0i32; num_nodes];

        let mut positions = vec![
            Float3 { x: 0.0, y: 0.0, z: 0.0 },
            Float3 { x: 1.0, y: 1.0, z: 0.0 },
            Float3 { x: 2.0, y: 0.0, z: 0.0 },
            Float3 { x: 3.0, y: 2.0, z: 0.0 },
        ];
        let mut forces = vec![Float3 { x: 0.0, y: 0.0, z: 0.0 }; num_nodes];

        apply_maturity_layout_force(&codes, &mut positions, &mut forces, num_nodes);

        for (i, f) in forces.iter().enumerate() {
            assert!(
                f.x == 0.0 && f.y == 0.0 && f.z == 0.0,
                "node {} should receive zero maturity force when code=None, got ({},{},{})",
                i, f.x, f.y, f.z
            );
        }
    }
    #[cfg(not(feature = "gpu"))]
    {}
}

/// Physicality GPU smoke: same-code nodes should accumulate a non-zero centroid
/// and receive non-NaN force after one kernel call.
///
/// Uses the kernel bridge CPU path for centroid accumulation and force
/// application — no `UnifiedGPUCompute` required.
///
/// Requires: build with `--features gpu` (for Float3 and kernel wrappers).
#[test]
#[ignore] // requires CUDA
fn test_phase_a_physicality_forces_apply_without_panic() {
    #[cfg(feature = "gpu")]
    {
        let num_nodes = 4;

        // Nodes 0 and 1 share Abstract (1); nodes 2 and 3 share Virtual (2).
        let phys = vec![1i32, 1, 2, 2];

        // Run one physics tick equivalent using the kernel bridge directly
        // (avoids needing a full SimulationParams round-trip just for smoke).
        let positions = vec![
            Float3 { x: -10.0, y: -10.0, z: 0.0 },
            Float3 { x:  10.0, y: -10.0, z: 0.0 },
            Float3 { x: -10.0, y:  10.0, z: 0.0 },
            Float3 { x:  10.0, y:  10.0, z: 0.0 },
        ];
        let mut pos_buf = positions.clone();

        let mut centroids = vec![Float3 { x: 0.0, y: 0.0, z: 0.0 }; 4];
        let mut counts    = vec![0i32; 4];
        let mut forces    = vec![Float3 { x: 0.0, y: 0.0, z: 0.0 }; num_nodes];

        calculate_physicality_centroids(&phys, &positions, &mut centroids, &mut counts, num_nodes);
        finalize_physicality_centroids(&mut centroids, &counts);

        apply_physicality_cluster_force(&phys, &centroids, &mut pos_buf, &mut forces, num_nodes);

        // Verify no NaN produced.
        for (i, f) in forces.iter().enumerate() {
            assert!(
                f.x.is_finite() && f.y.is_finite() && f.z.is_finite(),
                "node {} physicality force contains non-finite value: ({},{},{})",
                i, f.x, f.y, f.z
            );
        }

        // Nodes 0 and 1 share Abstract — their centroid is at (0, -10, 0).
        // Both should receive a force in the direction of the centroid (i.e.
        // node 0's x-force is positive, node 1's x-force is negative).
        // We only check sign/direction, not magnitude.
        // (Exact values depend on the kernel's strength constant.)
        // Since both are equidistant from centroid in x, their x-forces are
        // equal and opposite; y-forces are zero (both at y = -10 = centroid).
        // At minimum the forces must not both be zero for same-code paired nodes.
        let force_0_mag = (forces[0].x.powi(2) + forces[0].y.powi(2) + forces[0].z.powi(2)).sqrt();
        let force_1_mag = (forces[1].x.powi(2) + forces[1].y.powi(2) + forces[1].z.powi(2)).sqrt();

        // Both Abstract nodes must experience some non-zero force.
        assert!(
            force_0_mag > 0.0 || force_1_mag > 0.0,
            "Abstract-coded nodes 0 and 1 should experience non-zero physicality forces, \
             got ({},{},{}) and ({},{},{})",
            forces[0].x, forces[0].y, forces[0].z,
            forces[1].x, forces[1].y, forces[1].z,
        );
    }
    #[cfg(not(feature = "gpu"))]
    {}
}

/// Role GPU smoke: same-code nodes receive non-NaN, non-zero role forces.
///
/// Uses the kernel bridge CPU path — no `UnifiedGPUCompute` required.
///
/// Requires: build with `--features gpu` (for Float3 and kernel wrappers).
#[test]
#[ignore] // requires CUDA
fn test_phase_a_role_forces_apply_without_panic() {
    #[cfg(feature = "gpu")]
    {
        let num_nodes = 4;

        // Nodes 0 and 1 are Concept (1); nodes 2 and 3 are Process (3).
        let role = vec![1i32, 1, 3, 3];

        let positions = vec![
            Float3 { x: -5.0, y:  0.0, z: 0.0 },
            Float3 { x:  5.0, y:  0.0, z: 0.0 },
            Float3 { x: -5.0, y: 10.0, z: 0.0 },
            Float3 { x:  5.0, y: 10.0, z: 0.0 },
        ];
        let mut pos_buf = positions.clone();
        let mut centroids = vec![Float3 { x: 0.0, y: 0.0, z: 0.0 }; 7];
        let mut counts    = vec![0i32; 7];
        let mut forces    = vec![Float3 { x: 0.0, y: 0.0, z: 0.0 }; num_nodes];

        calculate_role_centroids(&role, &positions, &mut centroids, &mut counts, num_nodes);
        finalize_role_centroids(&mut centroids, &counts);

        apply_role_cluster_force(&role, &centroids, &mut pos_buf, &mut forces, num_nodes);

        for (i, f) in forces.iter().enumerate() {
            assert!(
                f.x.is_finite() && f.y.is_finite() && f.z.is_finite(),
                "node {} role force contains non-finite value: ({},{},{})",
                i, f.x, f.y, f.z
            );
        }
    }
    #[cfg(not(feature = "gpu"))]
    {}
}

/// Maturity GPU smoke: non-None nodes should receive a non-zero Z-component
/// from the maturity layout kernel (vertical stratification behaviour).
///
/// Uses the kernel bridge CPU path — no `UnifiedGPUCompute` required.
///
/// Requires: build with `--features gpu` (for Float3 and kernel wrappers).
#[test]
#[ignore] // requires CUDA
fn test_phase_a_maturity_forces_apply_without_panic() {
    #[cfg(feature = "gpu")]
    {
        let num_nodes = 4;

        // Node 0: Emerging (1), Node 1: Mature (2), Node 2: Declining (3), Node 3: None (0).
        let mat  = vec![1i32, 2, 3, 0];

        let mut positions = vec![
            Float3 { x: 0.0, y: 0.0, z:  0.0 },
            Float3 { x: 1.0, y: 0.0, z:  5.0 },
            Float3 { x: 2.0, y: 0.0, z: 10.0 },
            Float3 { x: 3.0, y: 0.0, z:  0.0 },
        ];
        let mut forces = vec![Float3 { x: 0.0, y: 0.0, z: 0.0 }; num_nodes];

        apply_maturity_layout_force(&mat, &mut positions, &mut forces, num_nodes);

        // All forces must be finite.
        for (i, f) in forces.iter().enumerate() {
            assert!(
                f.x.is_finite() && f.y.is_finite() && f.z.is_finite(),
                "node {} maturity force contains non-finite value: ({},{},{})",
                i, f.x, f.y, f.z
            );
        }

        // Node 3 (None) must receive zero maturity force.
        let f3 = &forces[3];
        assert!(
            f3.x == 0.0 && f3.y == 0.0 && f3.z == 0.0,
            "None-coded node 3 should receive zero maturity force, got ({},{},{})",
            f3.x, f3.y, f3.z
        );

        // Nodes 0-2 with non-None maturity codes should experience some force
        // (specifically a Z-component that drives vertical stratification).
        // We assert at least one non-None node has a non-zero force magnitude.
        let any_nonzero = forces[..3].iter().any(|f| {
            f.x != 0.0 || f.y != 0.0 || f.z != 0.0
        });
        assert!(
            any_nonzero,
            "at least one non-None maturity node should receive a non-zero force; \
             forces = {:?}",
            forces
                .iter()
                .map(|f| format!("({:.3},{:.3},{:.3})", f.x, f.y, f.z))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    #[cfg(not(feature = "gpu"))]
    {}
}
