//! Typed Finding for the IOSP dimension.
//!
//! An `IospFinding` represents a function-level Integration/Operation
//! Segregation violation: a function that mixes own-call delegation with
//! its own logic. The Finding carries the per-occurrence locations
//! (logic statements + own calls) so reporters can render rich,
//! actionable detail.

use crate::domain::Finding;

/// A single logic occurrence inside a function body.
///
/// `kind` is one of the canonical labels emitted by the IOSP visitor:
/// `if`, `match`, `for`, `while`, `loop`, `arithmetic`, `boolean_op`, `?`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicLocation {
    pub kind: String,
    pub line: usize,
}

/// A single own-call inside a function body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallLocation {
    pub name: String,
    pub line: usize,
}

/// IOSP violation finding: a function that contains both logic and own calls.
#[derive(Debug, Clone, PartialEq)]
pub struct IospFinding {
    /// Common file/line/rule_id/severity/suppressed metadata.
    /// `common.dimension` is always `Dimension::Iosp`.
    pub common: Finding,
    /// Logic statements found inside the function body.
    pub logic_locations: Vec<LogicLocation>,
    /// Own-function calls found inside the function body.
    pub call_locations: Vec<CallLocation>,
    /// Optional refactoring effort score (higher = bigger refactor needed).
    pub effort_score: Option<f64>,
}
