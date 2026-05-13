//! Typed Finding for the SRP dimension.
//!
//! SRP findings come in three flavors: low-cohesion structs (LCOM4),
//! over-long modules (with cluster-detected independent responsibilities),
//! and parameter-count smells. Common metadata in `common`; per-variant
//! detail in `SrpFindingDetails`.

use crate::domain::Finding;

/// Sub-category of SRP finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SrpFindingKind {
    StructCohesion,
    ModuleLength,
    ParameterCount,
    /// Structural binary check on the SRP side: BTC (broken trait
    /// contract), SLM (selfless method), NMS (needless mut self).
    /// The exact rule lives in `common.rule_id` and `details::Structural`.
    Structural,
}

/// One responsibility cluster identified by LCOM4 — a connected
/// component of methods sharing field accesses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponsibilityCluster {
    pub methods: Vec<String>,
    pub fields: Vec<String>,
}

/// Per-variant detail for an SRP finding.
#[derive(Debug, Clone, PartialEq)]
pub enum SrpFindingDetails {
    StructCohesion {
        struct_name: String,
        lcom4: usize,
        field_count: usize,
        method_count: usize,
        fan_out: usize,
        composite_score: f64,
        clusters: Vec<ResponsibilityCluster>,
    },
    ModuleLength {
        module: String,
        production_lines: usize,
        independent_clusters: usize,
        /// Each inner vec is one cluster (list of function names).
        cluster_names: Vec<Vec<String>>,
        length_score: f64,
    },
    ParameterCount {
        function_name: String,
        parameter_count: usize,
    },
    /// Structural binary check (BTC/SLM/NMS). `code` is the short
    /// identifier emitted in reports (e.g. `BTC`); `detail` is the
    /// human-readable explanation.
    Structural {
        item_name: String,
        code: String,
        detail: String,
    },
}

/// SRP finding — struct cohesion, module length, or parameter-count smell.
#[derive(Debug, Clone, PartialEq)]
pub struct SrpFinding {
    /// Common metadata. `common.dimension == Dimension::Srp`.
    pub common: Finding,
    /// Which SRP sub-category triggered.
    pub kind: SrpFindingKind,
    /// Per-variant detail.
    pub details: SrpFindingDetails,
}
