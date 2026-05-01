/// SimParams validation trait per ADR-070 D1.1.
///
/// Called by ForceComputeActor before every `cudaMemcpyToSymbol(c_params, ...)`.
/// On rejection: GPU upload is skipped, previous valid params stay, audit event fired.
pub trait SimParamsValidation {
    fn validate_for_gpu(&self) -> Result<(), Vec<String>>;
}

/// Standalone validation for GPU-bound SimParams fields.
///
/// Validates the raw numeric values that will be uploaded to the GPU.
/// This is separate from the existing `SimulationParams::validate()` which
/// validates the higher-level config struct.
pub fn validate_gpu_params(params: &GpuParamsView) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    // ADR-070 D1.1: dt ∈ [0.001, 0.1]
    if params.dt < 0.001 || params.dt > 0.1 {
        errors.push(format!("dt must be in [0.001, 0.1], got {}", params.dt));
    }

    // damping ∈ (0.0, 1.0)
    if params.damping <= 0.0 || params.damping >= 1.0 {
        errors.push(format!("damping must be in (0.0, 1.0), got {}", params.damping));
    }

    // spring_k >= 0
    if params.spring_k < 0.0 {
        errors.push(format!("spring_k must be >= 0, got {}", params.spring_k));
    }

    // repel_k >= 0
    if params.repel_k < 0.0 {
        errors.push(format!("repel_k must be >= 0, got {}", params.repel_k));
    }

    // max_force > 0
    if params.max_force <= 0.0 {
        errors.push(format!("max_force must be > 0, got {}", params.max_force));
    }

    // max_velocity > 0
    if params.max_velocity <= 0.0 {
        errors.push(format!("max_velocity must be > 0, got {}", params.max_velocity));
    }

    // gravity magnitude <= 100 (sanity ceiling)
    if params.gravity.abs() > 100.0 {
        errors.push(format!(
            "gravity magnitude must be <= 100, got {}",
            params.gravity.abs()
        ));
    }

    // rest_length > 0
    if params.rest_length <= 0.0 {
        errors.push(format!("rest_length must be > 0, got {}", params.rest_length));
    }

    // All fields finite (NaN / ±Inf check)
    let fields: &[(&str, f32)] = &[
        ("dt", params.dt),
        ("damping", params.damping),
        ("spring_k", params.spring_k),
        ("repel_k", params.repel_k),
        ("max_force", params.max_force),
        ("max_velocity", params.max_velocity),
        ("gravity", params.gravity),
        ("rest_length", params.rest_length),
        ("center_gravity_k", params.center_gravity_k),
        ("temperature", params.temperature),
    ];
    for &(name, value) in fields {
        if !value.is_finite() {
            errors.push(format!("{name} must be finite, got {value}"));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// View struct for GPU-bound parameters, decoupled from the monolith's SimParams.
/// Constructed by the caller from whichever source struct they have.
#[derive(Debug, Clone, Copy)]
pub struct GpuParamsView {
    pub dt: f32,
    pub damping: f32,
    pub spring_k: f32,
    pub repel_k: f32,
    pub max_force: f32,
    pub max_velocity: f32,
    pub gravity: f32,
    pub rest_length: f32,
    pub center_gravity_k: f32,
    pub temperature: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_params() -> GpuParamsView {
        GpuParamsView {
            dt: 0.016,
            damping: 0.85,
            spring_k: 0.5,
            repel_k: 100.0,
            max_force: 10.0,
            max_velocity: 5.0,
            gravity: 0.0001,
            rest_length: 1.0,
            center_gravity_k: 0.01,
            temperature: 1.0,
        }
    }

    #[test]
    fn valid_params_pass() {
        assert!(validate_gpu_params(&valid_params()).is_ok());
    }

    #[test]
    fn nan_dt_rejected() {
        let mut p = valid_params();
        p.dt = f32::NAN;
        let err = validate_gpu_params(&p).unwrap_err();
        assert!(err.iter().any(|e| e.contains("dt")));
    }

    #[test]
    fn negative_spring_k_rejected() {
        let mut p = valid_params();
        p.spring_k = -1.0;
        let err = validate_gpu_params(&p).unwrap_err();
        assert!(err.iter().any(|e| e.contains("spring_k")));
    }

    #[test]
    fn gravity_over_100_rejected() {
        let mut p = valid_params();
        p.gravity = -150.0;
        let err = validate_gpu_params(&p).unwrap_err();
        assert!(err.iter().any(|e| e.contains("gravity")));
    }

    #[test]
    fn zero_rest_length_rejected() {
        let mut p = valid_params();
        p.rest_length = 0.0;
        let err = validate_gpu_params(&p).unwrap_err();
        assert!(err.iter().any(|e| e.contains("rest_length")));
    }

    #[test]
    fn damping_at_boundary_rejected() {
        let mut p = valid_params();
        p.damping = 1.0;
        assert!(validate_gpu_params(&p).is_err());

        p.damping = 0.0;
        assert!(validate_gpu_params(&p).is_err());
    }

    #[test]
    fn multiple_errors_collected() {
        let mut p = valid_params();
        p.dt = -1.0;
        p.spring_k = -5.0;
        p.max_velocity = 0.0;
        let err = validate_gpu_params(&p).unwrap_err();
        assert!(err.len() >= 3);
    }
}
