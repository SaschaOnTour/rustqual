//! Per-section View structs for the HTML reporter.
//!
//! Each `build_*` projects its input slice into one of these structs —
//! pure data, no markup. The matching `format_*_section` helper then
//! takes the View and produces the HTML string. publish orchestrates.

// ── IOSP ───────────────────────────────────────────────────────────

/// Finding-side IOSP data: for each violation, the joined detail rows.
/// Indexed by (file, line) so publish can match against function data.
pub struct HtmlIospView {
    pub findings: Vec<HtmlIospFindingRow>,
}

pub struct HtmlIospFindingRow {
    pub file: String,
    pub line: usize,
    pub logic_summary: String, // pre-joined "if (line N), for (line M)"
    pub call_summary: String,  // pre-joined "helper (line N)"
}

/// Function-side IOSP data: per-function classification + severity.
pub struct HtmlIospDataView {
    pub violations: Vec<HtmlIospFunctionRow>,
}

pub struct HtmlIospFunctionRow {
    pub qualified_name: String,
    pub file: String,
    pub line: usize,
    pub severity_class: &'static str, // "severity-high" / "" / etc.
    pub severity_label: &'static str, // "High" / "—" / etc.
    pub effort: String,               // "{e:.1}" or empty
}

// ── Complexity ────────────────────────────────────────────────────

/// Finding-side complexity data: just the (file, line, kind) keys of
/// flagged functions. publish uses this as a filter against the
/// data-side function rows.
pub struct HtmlComplexityView {
    pub flagged_keys: Vec<HtmlComplexityKey>,
}

pub struct HtmlComplexityKey {
    pub file: String,
    pub line: usize,
    /// True if the finding is a magic-number (which is per-function
    /// regardless of line), so any function in that file is flagged.
    pub is_magic_number: bool,
}

/// Function-side complexity data: per-function metrics + computed
/// issue summary. publish filters by flagged_keys.
pub struct HtmlComplexityDataView {
    pub functions: Vec<HtmlComplexityFunctionRow>,
}

pub struct HtmlComplexityFunctionRow {
    pub qualified_name: String,
    pub file: String,
    pub line: usize,
    pub cognitive: usize,
    pub cyclomatic: usize,
    pub max_nesting: usize,
    pub function_lines: usize,
    /// Pre-computed issue string ("magic: 42 (line 5); 1 unsafe; 2unwrap" or "—").
    pub issue_summary: String,
    pub suppressed: bool,
    pub complexity_suppressed: bool,
}

// ── DRY ───────────────────────────────────────────────────────────

pub struct HtmlDryView {
    pub duplicate_groups: Vec<DryGroupRow>,
    pub fragment_groups: Vec<DryGroupRow>,
    pub repeated_match_groups: Vec<DryGroupRow>,
    pub dead_code: Vec<DeadCodeRow>,
    pub boilerplate: Vec<BoilerplateRow>,
    pub wildcards: Vec<WildcardRow>,
}

// DRY row types live in `report::projections::dry` and are shared.
pub(crate) use crate::adapters::report::projections::dry::{
    BoilerplateRow, DeadCodeRow, DryGroupRow, ParticipantRow, WildcardRow,
};

// ── SRP ───────────────────────────────────────────────────────────

pub struct HtmlSrpView {
    pub struct_warnings: Vec<SrpStructRow>,
    pub module_warnings: Vec<SrpModuleRow>,
    pub param_warnings: Vec<SrpParamRow>,
    pub structural_rows: Vec<HtmlStructuralRow>,
}

// SRP row types live in `report::projections::srp` and are re-exported below.
pub(crate) use crate::adapters::report::projections::srp::{
    SrpModuleRow, SrpParamRow, SrpStructRow,
};

// ── Coupling ─────────────────────────────────────────────────────

pub struct HtmlCouplingView {
    pub cycle_paths: Vec<Vec<String>>,
    pub sdp_violations: Vec<SdpViolationRow>,
    pub structural_rows: Vec<HtmlStructuralRow>,
}

// SDP violation row shared via `report::projections::coupling`.
pub(crate) use crate::adapters::report::projections::coupling::SdpViolationRow;

pub struct HtmlCouplingDataView {
    pub modules: Vec<HtmlCouplingModuleRow>,
}

pub struct HtmlCouplingModuleRow {
    pub name: String,
    pub afferent: usize,
    pub efferent: usize,
    pub instability: f64,
    pub suppressed: bool,
}

// ── Structural (cross-dim, shared between SrpView and CouplingView) ─
// `StructuralRow` lives in `report::projections::srp` and is shared
// across reporters. Aliased here for the html sections to use without
// further qualification.

pub(crate) use crate::adapters::report::projections::srp::StructuralRow as HtmlStructuralRow;

// ── Test Quality ─────────────────────────────────────────────────

pub struct HtmlTqView {
    pub warnings: Vec<TqRow>,
}

// TQ row shared via `report::projections::tq`.
pub(crate) use crate::adapters::report::projections::tq::TqRow;

// ── Architecture ────────────────────────────────────────────────

pub struct HtmlArchitectureView {
    pub findings: Vec<HtmlArchitectureRow>,
}

pub struct HtmlArchitectureRow {
    pub rule_id: String,
    pub file: String,
    pub line: usize,
    pub message: String,
}
