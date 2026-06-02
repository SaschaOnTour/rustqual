//! JSON reporter tests, split into focused sub-files (each ≤ the SRP
//! file-length cap); shared imports + the `json_value`/`function_named`
//! helpers live here and reach the sub-modules via `use super::*`.

pub(super) use crate::adapters::analyzers::iosp::{
    CallOccurrence, Classification, ComplexityMetrics, FunctionAnalysis, LogicOccurrence,
};
pub(super) use crate::adapters::report::test_support::{make_analysis, make_result};
pub(super) use crate::report::json::*;
pub(super) use crate::report::AnalysisResult;

mod srp_complexity_severity;
mod summary_fields_and_orphan;
mod violations_and_dups;

/// Build the JSON report for `analysis` and parse it back into a `Value`.
pub(super) fn json_value(analysis: &AnalysisResult) -> serde_json::Value {
    let json = crate::report::json::build_json_string(analysis);
    serde_json::from_str(&json).expect("reporter must emit valid JSON")
}

/// The `functions[]` entry named `name`, panicking if absent.
pub(super) fn function_named<'a>(v: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    v["functions"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"] == name))
        .unwrap_or_else(|| panic!("function `{name}` present"))
}
