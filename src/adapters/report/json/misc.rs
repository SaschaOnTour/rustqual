//! JSON section builders for the smaller dimensions: TQ, Architecture,
//! orphan-suppressions, plus the summary projection.

use super::super::json_types::{
    JsonArchitectureFinding, JsonOrphanSuppression, JsonSummary, JsonTqWarning,
};
use super::super::AnalysisResult;
use crate::domain::findings::{ArchitectureFinding, TqFinding};

pub(super) fn build_summary(s: &super::super::Summary) -> JsonSummary {
    JsonSummary {
        total: s.total,
        integrations: s.integrations,
        operations: s.operations,
        violations: s.violations,
        trivial: s.trivial,
        suppressed: s.suppressed,
        all_suppressions: s.all_suppressions,
        iosp_score: s.iosp_score,
        quality_score: s.quality_score,
        complexity_warnings: s.complexity_warnings,
        magic_number_warnings: s.magic_number_warnings,
        coupling_warnings: s.coupling_warnings,
        coupling_cycles: s.coupling_cycles,
        duplicate_groups: s.duplicate_groups,
        dead_code_warnings: s.dead_code_warnings,
        fragment_groups: s.fragment_groups,
        boilerplate_warnings: s.boilerplate_warnings,
        srp_struct_warnings: s.srp_struct_warnings,
        srp_module_warnings: s.srp_module_warnings,
        srp_param_warnings: s.srp_param_warnings,
        nesting_depth_warnings: s.nesting_depth_warnings,
        function_length_warnings: s.function_length_warnings,
        unsafe_warnings: s.unsafe_warnings,
        error_handling_warnings: s.error_handling_warnings,
        wildcard_import_warnings: s.wildcard_import_warnings,
        sdp_violations: s.sdp_violations,
        tq_no_assertion_warnings: s.tq_no_assertion_warnings,
        tq_no_sut_warnings: s.tq_no_sut_warnings,
        tq_untested_warnings: s.tq_untested_warnings,
        tq_uncovered_warnings: s.tq_uncovered_warnings,
        tq_untested_logic_warnings: s.tq_untested_logic_warnings,
        structural_srp_warnings: s.structural_srp_warnings,
        structural_coupling_warnings: s.structural_coupling_warnings,
        repeated_match_groups: s.repeated_match_groups,
        architecture_warnings: s.architecture_warnings,
        orphan_suppressions: s.orphan_suppressions,
        dimension_scores: s.dimension_scores,
        suppression_ratio_exceeded: s.suppression_ratio_exceeded,
    }
}

pub(super) fn build_tq(findings: &[TqFinding]) -> Vec<JsonTqWarning> {
    findings
        .iter()
        .map(|f| JsonTqWarning {
            file: f.common.file.clone(),
            line: f.common.line,
            function_name: f.function_name.clone(),
            kind: f.kind.meta().json_kind.to_string(),
            suppressed: f.common.suppressed,
        })
        .collect()
}

pub(super) fn build_orphans(analysis: &AnalysisResult) -> Vec<JsonOrphanSuppression> {
    analysis
        .orphan_suppressions
        .iter()
        .map(|w| JsonOrphanSuppression {
            file: w.file.clone(),
            line: w.line,
            dimensions: w.dimensions.iter().map(|d| format!("{d}")).collect(),
            reason: w.reason.clone(),
        })
        .collect()
}

pub(super) fn build_architecture(findings: &[ArchitectureFinding]) -> Vec<JsonArchitectureFinding> {
    findings
        .iter()
        .map(|f| JsonArchitectureFinding {
            file: f.common.file.clone(),
            line: f.common.line,
            rule_id: f.common.rule_id.clone(),
            severity: f.common.severity.levels().lowercase.to_string(),
            message: f.common.message.clone(),
            suppressed: f.common.suppressed,
        })
        .collect()
}
