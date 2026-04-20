mod cycles;
mod graph;
pub(crate) mod metrics;
pub(crate) mod sdp;

/// Module-level dependency graph.
#[derive(Debug, Clone, Default)]
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
    /// Stable Dependencies Principle violations. Populated by
    /// [`populate_sdp_violations`] after coupling suppressions have been
    /// applied, so each violation can be tagged correctly at creation.
    pub sdp_violations: Vec<sdp::SdpViolation>,
    /// Module-level dependency graph, retained so SDP can be computed
    /// after the pipeline has marked coupling suppressions.
    pub graph: ModuleGraph,
}

/// Run coupling analysis on parsed files. The resulting
/// `sdp_violations` vector is empty until [`populate_sdp_violations`]
/// runs — callers must apply coupling suppressions to `metrics` first
/// so each SDP violation can inherit the correct suppressed state.
/// Integration: orchestrates graph building, metrics computation, and cycle detection.
pub fn analyze_coupling(parsed: &[(String, String, syn::File)]) -> CouplingAnalysis {
    let graph = graph::build_module_graph(parsed);
    let metrics = metrics::compute_coupling_metrics(&graph);
    let cycles = cycles::detect_cycles(&graph);
    CouplingAnalysis {
        metrics,
        cycles,
        sdp_violations: Vec::new(),
        graph,
    }
}

/// Fill `sdp_violations` using the coupling analysis's graph and
/// (already-marked) metrics. Pipeline calls this after
/// `mark_coupling_suppressions` so violations inherit the correct
/// suppressed state at creation time.
/// Trivial: delegates to `sdp::check_sdp`.
pub fn populate_sdp_violations(analysis: &mut CouplingAnalysis) {
    analysis.sdp_violations = sdp::check_sdp(&analysis.graph, &analysis.metrics);
}

#[cfg(test)]
mod tests;
