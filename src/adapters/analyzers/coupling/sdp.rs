/// A violation of the Stable Dependencies Principle (SDP).
/// A stable module (low instability) depends on an unstable module (high instability).
#[derive(Debug, Clone)]
pub struct SdpViolation {
    /// The depending module (more stable).
    pub from_module: String,
    /// The depended-upon module (less stable).
    pub to_module: String,
    /// Instability of the depending module.
    pub from_instability: f64,
    /// Instability of the depended-upon module.
    pub to_instability: f64,
    /// Whether this violation is suppressed (either module has a coupling suppression).
    pub suppressed: bool,
}

/// Check the Stable Dependencies Principle: for each dependency edge A→B,
/// if A is more stable (lower instability) than B, it's a violation.
/// Operation: iteration + comparison logic, no own calls.
pub(super) fn check_sdp(
    graph: &super::ModuleGraph,
    metrics: &[super::CouplingMetrics],
) -> Vec<SdpViolation> {
    let instability: std::collections::HashMap<&str, f64> = metrics
        .iter()
        .map(|m| (m.module_name.as_str(), m.instability))
        .collect();

    let mut violations = Vec::new();
    for (from_idx, deps) in graph.forward.iter().enumerate() {
        let from_name = &graph.modules[from_idx];
        let from_inst = instability.get(from_name.as_str()).copied().unwrap_or(0.0);
        for &to_idx in deps {
            let to_name = &graph.modules[to_idx];
            let to_inst = instability.get(to_name.as_str()).copied().unwrap_or(0.0);
            // SDP violation: stable module depends on less stable module
            if from_inst < to_inst {
                violations.push(SdpViolation {
                    from_module: from_name.clone(),
                    to_module: to_name.clone(),
                    from_instability: from_inst,
                    to_instability: to_inst,
                    suppressed: false,
                });
            }
        }
    }
    violations
}
