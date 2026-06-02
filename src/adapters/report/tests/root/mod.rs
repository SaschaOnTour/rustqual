//! Reporter `root` tests (summary counts, JSON projection, quality-score
//! formulas). Split into focused sub-files (each ≤ the SRP file-length cap);
//! shared imports reach the sub-modules via `use super::*`.

pub(super) use crate::adapters::analyzers::iosp::{
    CallOccurrence, Classification, ComplexityMetrics, FunctionAnalysis, LogicOccurrence,
};
pub(super) use crate::adapters::report::test_support::make_result;
pub(super) use crate::report::*;

mod quality_score;
mod summary_and_json;
