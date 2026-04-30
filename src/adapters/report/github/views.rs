//! Per-section View structs for the github reporter. Each row holds
//! the data needed to format one `::level file=,line=::msg`
//! annotation; `publish` formats them into the final block.
//!
//! `GithubDetailRow<D>` + `GithubDetailListView<D>` is the shared shape
//! for the three details-bearing dimensions (DRY, SRP, Coupling) — the
//! payload is just the dim-specific `details` enum. Type aliases
//! preserve the public per-dim names while collapsing the redundant
//! row-struct definitions.

use crate::domain::findings::{
    ComplexityFindingKind, CouplingFindingDetails, DryFindingDetails, SrpFindingDetails,
};
use crate::domain::Severity;

pub struct GithubIospView {
    pub(crate) rows: Vec<GithubIospRow>,
}

pub struct GithubIospRow {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) severity: Severity,
    pub(crate) logic_locations: Vec<(String, usize)>,
    pub(crate) call_locations: Vec<(String, usize)>,
    pub(crate) effort_score: Option<f64>,
}

pub struct GithubComplexityView {
    pub(crate) rows: Vec<GithubComplexityRow>,
}

pub struct GithubComplexityRow {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) kind: ComplexityFindingKind,
    pub(crate) message: String,
}

pub struct GithubDetailRow<D> {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) severity: Severity,
    pub(crate) details: D,
    pub(crate) fallback_message: String,
}

pub struct GithubDetailListView<D> {
    pub(crate) rows: Vec<GithubDetailRow<D>>,
}

pub type GithubDryRow = GithubDetailRow<DryFindingDetails>;
pub type GithubDryView = GithubDetailListView<DryFindingDetails>;

pub type GithubSrpRow = GithubDetailRow<SrpFindingDetails>;
pub type GithubSrpView = GithubDetailListView<SrpFindingDetails>;

pub type GithubCouplingRow = GithubDetailRow<CouplingFindingDetails>;
pub type GithubCouplingView = GithubDetailListView<CouplingFindingDetails>;

pub struct GithubTqView {
    pub(crate) rows: Vec<GithubTqRow>,
}

pub struct GithubTqRow {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) severity: Severity,
    pub(crate) message: String,
}

pub struct GithubArchitectureView {
    pub(crate) rows: Vec<GithubArchitectureRow>,
}

pub struct GithubArchitectureRow {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) severity: Severity,
    pub(crate) rule_id: String,
    pub(crate) message: String,
}
