//! Per-module coupling state record.
//!
//! `ModuleCouplingRecord` carries the coupling metrics for **every**
//! analyzed module — also those without findings — so reporters can
//! render the full coupling table (afferent/efferent/instability per
//! module, dependency lists). Coupling findings (cycles, SDP violations,
//! threshold breaches) live on `CouplingFinding`.

/// Coupling metrics for a single module.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleCouplingRecord {
    /// Module name (e.g. `adapters::analyzers::iosp`).
    pub module_name: String,
    /// Afferent coupling: number of modules that depend on this one.
    pub afferent: usize,
    /// Efferent coupling: number of modules this one depends on.
    pub efferent: usize,
    /// Instability: `efferent / (afferent + efferent)`. Range `[0.0, 1.0]`.
    pub instability: f64,
    /// Names of modules that depend on this one (incoming edges).
    pub incoming: Vec<String>,
    /// Names of modules this one depends on (outgoing edges).
    pub outgoing: Vec<String>,
    /// Whether this module's coupling warnings are suppressed via
    /// `// qual:allow(coupling)`.
    pub suppressed: bool,
    /// Whether the module exceeds the configured coupling threshold.
    pub warning: bool,
}
