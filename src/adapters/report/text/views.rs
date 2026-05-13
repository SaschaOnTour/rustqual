//! Per-section View structs for the text reporter.
//!
//! Each `build_*` in [`super::TextReporter`] projects its input slice
//! into one of these structs — pure data, no markup. The matching
//! `format_*_section` helper then takes the View and produces the
//! actual output string. publish orchestrates: views in, formatted
//! sections out.
//!
//! Lives next to the section files so adding a field/section means
//! one file to edit (plus the consuming `format_*` helper).

pub(crate) use crate::adapters::report::projections::coupling::SdpViolationRow;
pub(crate) use crate::adapters::report::projections::srp::{
    SrpModuleRow, SrpParamRow, SrpStructRow, StructuralRow,
};

// ── Coupling ────────────────────────────────────────────────────────

/// Coupling-findings view: cycles + SDP violations + structural rows
/// (the latter feed the cross-dimension Structural section).
pub struct CouplingView {
    pub cycle_paths: Vec<Vec<String>>,
    pub sdp_violations: Vec<SdpViolationRow>,
    pub structural_rows: Vec<StructuralRow>,
}

/// Per-module coupling table row.
pub struct CouplingTableView {
    pub modules: Vec<ModuleRow>,
}

pub struct ModuleRow {
    pub name: String,
    pub afferent: usize,
    pub efferent: usize,
    pub instability: f64,
    pub suppressed: bool,
    pub warning: bool,
    pub incoming: Vec<String>,
    pub outgoing: Vec<String>,
}

// ── Structural (cross-dim) ──────────────────────────────────────────
// `StructuralRow` is now shared via `crate::adapters::report::projections::srp`.

// ── DRY ────────────────────────────────────────────────────────────

pub struct DryView {
    pub duplicate_groups: Vec<DryGroupRow>,
    pub fragment_groups: Vec<DryGroupRow>,
    pub dead_code: Vec<DeadCodeRow>,
    pub boilerplate: Vec<BoilerplateRow>,
    pub wildcards: Vec<WildcardRow>,
    pub repeated_match_groups: Vec<DryGroupRow>,
}

// DRY row types live in `report::projections::dry` and are shared.
pub(crate) use crate::adapters::report::projections::dry::{
    BoilerplateRow, DeadCodeRow, DryGroupRow, ParticipantRow, WildcardRow,
};

// ── SRP ────────────────────────────────────────────────────────────

pub struct SrpView {
    pub struct_warnings: Vec<SrpStructRow>,
    pub module_warnings: Vec<SrpModuleRow>,
    pub param_warnings: Vec<SrpParamRow>,
    pub structural_rows: Vec<StructuralRow>,
}

// Row types live in `report::projections::srp` and are re-exported above.

// ── Test Quality ───────────────────────────────────────────────────

pub struct TqView {
    pub warnings: Vec<TqRow>,
}

// TQ row shared via `report::projections::tq`.
pub(crate) use crate::adapters::report::projections::tq::TqRow;

// ── Architecture ──────────────────────────────────────────────────

pub struct ArchitectureView {
    pub findings: Vec<ArchitectureRow>,
}

pub struct ArchitectureRow {
    pub file: String,
    pub line: usize,
    pub rule_id: String,
    pub message: String,
}
