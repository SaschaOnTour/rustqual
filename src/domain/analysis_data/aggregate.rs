//! `AnalysisData` — typed state-of-codebase aggregate.
//!
//! Counterpart to `AnalysisFindings`. Where `AnalysisFindings` carries
//! _what's wrong_, `AnalysisData` carries _what is_: per-function
//! records (used by IOSP and Complexity dimensions) and per-module
//! coupling records (used by the Coupling dimension). Reporters that
//! emit dashboards (HTML, text-verbose, JSON) consume both; reporters
//! that emit only findings (AI, github, findings_list, SARIF) consume
//! `AnalysisFindings` only.
//!
//! DRY, SRP, TQ and Architecture have no state record today — they're
//! findings-only dimensions. When a future need surfaces, the new
//! state record + corresponding `AnalysisReporter` method are added
//! together so every reporter is forced to address them.

use super::coupling_record::ModuleCouplingRecord;
use super::function_record::FunctionRecord;

/// All state-of-codebase data of an analysis run.
///
/// Layout is flat — IOSP and Complexity dimensions both consume the
/// `functions` slice from different perspectives, so a shared field
/// avoids duplicating identity data across per-dimension containers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnalysisData {
    /// Per-function records. IOSP renders the classification view;
    /// Complexity renders the metrics view.
    pub functions: Vec<FunctionRecord>,
    /// Per-module coupling records — every analyzed module, also those
    /// without coupling findings, for the full coupling table.
    pub modules: Vec<ModuleCouplingRecord>,
}
