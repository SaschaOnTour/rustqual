//! Typed Finding for the Coupling dimension.
//!
//! Coupling produces three finding shapes: circular dependencies, SDP
//! (Stable-Dependency-Principle) violations, and per-module instability
//! threshold breaches. Per-module metric values (afferent/efferent/
//! instability) live on the dimension state struct, not here — the
//! Finding only carries what's needed to act on it.

use crate::domain::Finding;

/// Sub-category of Coupling finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CouplingFindingKind {
    Cycle,
    SdpViolation,
    ThresholdExceeded,
    /// Structural binary check on the Coupling side: OI (orphaned impl),
    /// SIT (single-impl trait), DEH (downcast escape hatch), IET
    /// (inconsistent error types). The exact rule lives in
    /// `common.rule_id` and `details::Structural`.
    Structural,
}

/// Per-variant detail for a Coupling finding.
#[derive(Debug, Clone, PartialEq)]
pub enum CouplingFindingDetails {
    Cycle {
        modules: Vec<String>,
    },
    SdpViolation {
        from_module: String,
        to_module: String,
        from_instability: f64,
        to_instability: f64,
    },
    ThresholdExceeded {
        module_name: String,
        afferent: usize,
        efferent: usize,
        instability: f64,
    },
    /// Structural binary check (OI/SIT/DEH/IET). `code` is the short
    /// identifier (e.g. `OI`); `detail` is the human-readable text.
    Structural {
        item_name: String,
        code: String,
        detail: String,
    },
}

/// Coupling finding — cycle, SDP violation, or threshold breach.
#[derive(Debug, Clone, PartialEq)]
pub struct CouplingFinding {
    /// Common metadata. `common.dimension == Dimension::Coupling`.
    pub common: Finding,
    /// Which Coupling sub-category triggered.
    pub kind: CouplingFindingKind,
    /// Per-variant detail.
    pub details: CouplingFindingDetails,
}
