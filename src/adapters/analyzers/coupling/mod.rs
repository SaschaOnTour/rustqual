mod cycles;
mod graph;
pub(crate) mod metrics;
pub(crate) mod sdp;

/// Module-level dependency graph.
#[derive(Debug, Clone)]
pub struct ModuleGraph {
    /// Module names, indexed by position.
    pub modules: Vec<String>,
    /// Forward adjacency list: module[i] depends on modules in forward[i].
    pub forward: Vec<Vec<usize>>,
}

/// Coupling metrics for a single module.
#[derive(Debug, Clone)]
pub struct CouplingMetrics {
    /// Module name.
    pub module_name: String,
    /// Afferent coupling (Ca): number of modules that depend on this one.
    pub afferent: usize,
    /// Efferent coupling (Ce): number of modules this one depends on.
    pub efferent: usize,
    /// Instability: Ce / (Ca + Ce). Range [0.0, 1.0].
    /// 0.0 = maximally stable, 1.0 = maximally unstable.
    pub instability: f64,
    /// Names of modules that depend on this one (incoming).
    pub incoming: Vec<String>,
    /// Names of modules this one depends on (outgoing).
    pub outgoing: Vec<String>,
    /// Whether this module's coupling warnings are suppressed via `// qual:allow(coupling)`.
    pub suppressed: bool,
    /// Whether this module exceeds coupling thresholds (set by pipeline).
    pub warning: bool,
}

/// A strongly connected component (cycle) in the module graph.
#[derive(Debug, Clone)]
pub struct CycleReport {
    /// Module names in the cycle (sorted alphabetically).
    pub modules: Vec<String>,
}

/// Full result of coupling analysis.
#[derive(Debug, Clone)]
pub struct CouplingAnalysis {
    /// Per-module coupling metrics.
    pub metrics: Vec<CouplingMetrics>,
    /// Circular dependencies (SCCs with 2+ modules).
    pub cycles: Vec<CycleReport>,
    /// Stable Dependencies Principle violations.
    pub sdp_violations: Vec<sdp::SdpViolation>,
}

/// Run coupling analysis on parsed files.
/// Integration: orchestrates graph building, metrics computation, and cycle detection.
pub fn analyze_coupling(parsed: &[(String, String, syn::File)]) -> CouplingAnalysis {
    let graph = graph::build_module_graph(parsed);
    let metrics = metrics::compute_coupling_metrics(&graph);
    let cycles = cycles::detect_cycles(&graph);
    let sdp_violations = sdp::check_sdp(&graph, &metrics);
    CouplingAnalysis {
        metrics,
        cycles,
        sdp_violations,
    }
}

/// Convert a file path to its top-level module name.
/// Operation: string manipulation logic, no own calls.
///
/// Examples:
/// - `main.rs` → `main`
/// - `config/mod.rs` → `config`
/// - `analyzer/types.rs` → `analyzer`
/// - `src/pipeline.rs` → `pipeline`
pub fn file_to_module(file_path: &str) -> String {
    let path = file_path.replace('\\', "/");
    let stripped = path.strip_prefix("src/").unwrap_or(&path);
    if let Some(slash_pos) = stripped.find('/') {
        stripped[..slash_pos].to_string()
    } else {
        stripped.strip_suffix(".rs").unwrap_or(stripped).to_string()
    }
}

#[cfg(test)]
mod tests;
