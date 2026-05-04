//! Unified finding type.
//!
//! Every dimension analyzer emits rich, dimension-specific diagnostic
//! structures internally (e.g. `FunctionAnalysis`, `CycleReport`,
//! `MatchLocation`). For cross-dimension aggregation, reporting, and
//! suppression, these are projected onto a single `Finding` shape that
//! carries only what every consumer needs: where the issue lives, which
//! rule raised it, a human-readable message, and how serious it is.
//!
//! The port `DimensionAnalyzer` produces `Vec<Finding>` so the Application
//! layer can treat all analyzers uniformly.

use crate::domain::{Dimension, Severity};

/// A projection of any analyzer's output into a single cross-dimension shape.
///
/// Construct directly via struct literal; all fields are `pub`. The
/// `Default` impl gives a neutral starting point for the common case
/// `Finding { file, line, dimension, rule_id, message, ..Default::default() }`
/// where column is unknown, severity is `Medium`, and the finding is not
/// suppressed.
///
/// Future: when a reporter needs rich finding-specific data beyond the
/// message (logic locations, similarity scores, per-adapter counts), add
/// `extra: FindingExtra` as a typed `pub enum` in domain — keeps the
/// domain layer free of serde_json/IO types. Today no reporter has this
/// need; the message string covers all cases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Path of the file the finding points at, normalised to forward slashes.
    /// Empty string for project-wide findings (e.g. a circular dependency
    /// spanning multiple files).
    pub file: String,
    /// 1-based line number, or 0 for project-wide findings.
    pub line: usize,
    /// 0-based column, or 0 when the source position carries no column.
    pub column: usize,
    /// The dimension that produced this finding.
    pub dimension: Dimension,
    /// Stable identifier for the specific rule, used in suppression strings
    /// and SARIF output. Convention: `"<dimension>/<rule>"` (snake_case),
    /// e.g. `"iosp/violation"`, `"dry/duplicate"`,
    /// `"architecture/call_parity/no_delegation"`.
    pub rule_id: String,
    /// Human-readable description. Short enough for a single report line.
    pub message: String,
    /// Severity bucket used to gate exit codes and order output.
    pub severity: Severity,
    /// Whether a suppression comment silences this finding. Findings with
    /// `suppressed == true` are still carried through the pipeline so that
    /// reports can show the suppression ratio; they do not count toward
    /// dimension scores.
    pub suppressed: bool,
}

impl Default for Finding {
    fn default() -> Self {
        Self {
            file: String::new(),
            line: 0,
            column: 0,
            dimension: Dimension::Iosp,
            rule_id: String::new(),
            message: String::new(),
            severity: Severity::Medium,
            suppressed: false,
        }
    }
}
