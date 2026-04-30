//! Per-function record consumed by the IOSP and Complexity dimensions.
//!
//! `FunctionRecord` is the typed state-of-codebase representation of a
//! single analyzed function. It carries everything reporters need to
//! render the per-function view — both classification (IOSP lens) and
//! raw complexity metrics (Complexity lens) — plus identity and a few
//! cross-cutting flags (`is_test`, `suppressed`, …).
//!
//! Violation-specific detail (logic_locations, call_locations) lives on
//! `IospFinding`, not here. `FunctionRecord` is state, not findings.

use crate::domain::Severity;

/// Classification of a function according to IOSP. Mirrors the analyzer's
/// `Classification` but without the `Violation` variant's per-occurrence
/// payload, which lives on `IospFinding` instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionClassification {
    Integration,
    Operation,
    Trivial,
    Violation,
}

/// One nesting hotspot inside a function (line + depth + construct kind).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestingHotspot {
    pub line: usize,
    pub nesting_depth: usize,
    pub construct: String,
}

/// One magic-number occurrence (line + value).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MagicNumberOccurrence {
    pub line: usize,
    pub value: String,
}

/// One logic occurrence — used for TQ-005 mapping (which logic lines are
/// covered).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicOccurrenceRecord {
    pub line: usize,
    pub kind: String,
}

/// Raw complexity metrics for a function (set when the function is
/// non-trivial; absent for `Trivial` functions).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComplexityMetricsRecord {
    pub cognitive_complexity: usize,
    pub cyclomatic_complexity: usize,
    pub max_nesting: usize,
    pub function_lines: usize,
    pub unsafe_blocks: usize,
    pub unwrap_count: usize,
    pub expect_count: usize,
    pub panic_count: usize,
    pub todo_count: usize,
    pub hotspots: Vec<NestingHotspot>,
    pub magic_numbers: Vec<MagicNumberOccurrence>,
    pub logic_occurrences: Vec<LogicOccurrenceRecord>,
}

/// State record for a single analyzed function.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionRecord {
    /// Function identity.
    pub name: String,
    pub file: String,
    pub line: usize,
    /// Pre-computed `MyStruct::method` or `free_fn` form.
    pub qualified_name: String,
    /// Parent impl type when the function is a method (e.g. `MyStruct`).
    pub parent_type: Option<String>,

    /// IOSP classification.
    pub classification: FunctionClassification,

    /// Severity of the IOSP violation, if any (mirrors what the IOSP
    /// finding carries — present here too so reporters that show the
    /// per-function lens don't need to cross-reference findings).
    pub severity: Option<Severity>,

    /// Raw complexity metrics. `None` for `Trivial` functions.
    pub complexity: Option<ComplexityMetricsRecord>,

    /// Function shape.
    pub parameter_count: usize,
    pub own_calls: Vec<String>,
    pub is_trait_impl: bool,
    pub is_test: bool,
    /// Refactoring effort estimate, populated for violations only.
    pub effort_score: Option<f64>,

    /// Suppression state — any-dimension `// qual:allow` matched here.
    pub suppressed: bool,
    /// Complexity-specific suppression state — `// qual:allow(complexity)`.
    pub complexity_suppressed: bool,
}
