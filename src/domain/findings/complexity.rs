//! Typed Finding for the Complexity dimension.
//!
//! A `ComplexityFinding` represents a function-level threshold breach:
//! cognitive/cyclomatic complexity, nesting depth, function length,
//! magic-number occurrences, unsafe blocks, or error-handling smells.
//! The `kind` discriminator selects which threshold was breached;
//! `metric_value` and `threshold` carry the numeric context.

use crate::domain::Finding;

/// Kind of complexity threshold that was breached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComplexityFindingKind {
    Cognitive,
    Cyclomatic,
    NestingDepth,
    FunctionLength,
    MagicNumber,
    Unsafe,
    ErrorHandling,
}

/// Per-kind static labels used by projection and reporters. Centralised
/// here so the kind→string mapping happens in one place — adding a new
/// kind variant is a single-match update, not a hunt across files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComplexityKindMeta {
    /// Stable rule_id (e.g. `complexity/cognitive`).
    pub rule_id: &'static str,
    /// AI-reporter category name (lowercase, snake_case).
    pub ai_category: &'static str,
    /// Human-readable description used in messages.
    pub description: &'static str,
}

impl ComplexityFindingKind {
    /// Static metadata for this kind: rule_id, AI category, description.
    pub const fn meta(self) -> ComplexityKindMeta {
        let (rule_id, ai_category, description) = match self {
            Self::Cognitive => (
                "complexity/cognitive",
                "cognitive_complexity",
                "cognitive complexity",
            ),
            Self::Cyclomatic => (
                "complexity/cyclomatic",
                "cyclomatic_complexity",
                "cyclomatic complexity",
            ),
            Self::NestingDepth => ("complexity/nesting", "nesting_depth", "nesting depth"),
            Self::FunctionLength => (
                "complexity/function_length",
                "long_function",
                "function length",
            ),
            Self::MagicNumber => ("complexity/magic_number", "magic_number", "magic number"),
            Self::Unsafe => ("complexity/unsafe", "unsafe_block", "unsafe blocks"),
            Self::ErrorHandling => (
                "complexity/error_handling",
                "error_handling",
                "error-handling smells",
            ),
        };
        ComplexityKindMeta {
            rule_id,
            ai_category,
            description,
        }
    }
}

/// One complexity hotspot detail (deepest-nesting occurrence).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComplexityHotspotDetail {
    pub line: usize,
    pub nesting_depth: usize,
    pub construct: String,
}

/// Complexity threshold-breach finding for a single function.
#[derive(Debug, Clone, PartialEq)]
pub struct ComplexityFinding {
    /// Common metadata. `common.dimension == Dimension::Complexity`.
    pub common: Finding,
    /// Which threshold this finding describes.
    pub kind: ComplexityFindingKind,
    /// The actual measured value (e.g. cognitive complexity = 23).
    pub metric_value: usize,
    /// The configured threshold the value exceeded.
    pub threshold: usize,
    /// Optional hotspot location for nesting-depth findings.
    pub hotspot: Option<ComplexityHotspotDetail>,
}
