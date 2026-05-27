//! build.rs for the webxr binary — ADR-090 Phase 3
//!
//! CUDA source files have moved to `crates/visionflow-gpu/src/cuda_sources/`.
//! Compilation and linking are now handled entirely by that crate's build.rs.
//!
//! This file is kept so Cargo does not complain about a missing build script;
//! it simply emits the GPU feature env-var check so existing conditional
//! compilation in webxr's own source files continues to work, and re-runs if
//! any relevant env vars change.

use std::env;

fn main() {
    let gpu_enabled = env::var("CARGO_FEATURE_GPU").is_ok();
    if !gpu_enabled {
        println!("cargo:warning=webxr build.rs: GPU feature not enabled");
    } else {
        println!("cargo:warning=webxr build.rs: GPU feature enabled — CUDA compilation delegated to visionflow-gpu crate");
    }

    // Notify Cargo to re-run if the GPU feature flag or CUDA env vars change.
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_GPU");
    println!("cargo:rerun-if-env-changed=CUDA_ARCH");
    println!("cargo:rerun-if-env-changed=DOCKER_ENV");
    println!("cargo:rerun-if-changed=build.rs");
}
