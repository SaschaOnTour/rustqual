//! SARIF reporter tests, split into focused sub-files (each ≤ the SRP
//! file-length cap); shared imports + the `sarif_result_by_rule` helper live
//! here and reach the sub-modules via `use super::*`.

pub(super) use crate::adapters::analyzers::iosp::{
    CallOccurrence, Classification, LogicOccurrence,
};
pub(super) use crate::adapters::report::test_support::{make_analysis, make_result};
pub(super) use crate::report::sarif::*;
pub(super) use crate::report::AnalysisResult;

mod core;
mod orphan_and_architecture;

/// The first SARIF result whose `ruleId` matches `rule_id`, panicking if none.
pub(super) fn sarif_result_by_rule(analysis: &AnalysisResult, rule_id: &str) -> serde_json::Value {
    let value = build_sarif_value(analysis);
    value["runs"][0]["results"]
        .as_array()
        .expect("results array")
        .iter()
        .find(|r| r["ruleId"] == rule_id)
        .unwrap_or_else(|| panic!("expected a SARIF result for rule {rule_id}"))
        .clone()
}
