use std::collections::BTreeMap;

use crate::adapters::analyzers::iosp::Severity;

#[derive(serde::Serialize)]
pub(crate) struct JsonOutput {
    pub(crate) summary: JsonSummary,
    pub(crate) functions: Vec<JsonFunction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) coupling: Option<JsonCoupling>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) duplicates: Vec<JsonDuplicateGroup>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) dead_code: Vec<JsonDeadCodeWarning>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) fragments: Vec<JsonFragmentGroup>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) boilerplate: Vec<JsonBoilerplateFind>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) wildcard_warnings: Vec<JsonWildcardWarning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) srp: Option<JsonSrp>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) tq_warnings: Vec<JsonTqWarning>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) structural_warnings: Vec<JsonStructuralWarning>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) repeated_matches: Vec<JsonRepeatedMatchGroup>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) orphan_suppressions: Vec<JsonOrphanSuppression>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) architecture_findings: Vec<JsonArchitectureFinding>,
}

/// Architecture-dimension finding (layer / forbidden / pattern /
/// trait_contract / call_parity). Mirrors `domain::Finding` with the
/// dimension implicit and severity stringified for JSON consumers.
#[derive(serde::Serialize)]
pub(crate) struct JsonArchitectureFinding {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) rule_id: String,
    pub(crate) severity: String,
    pub(crate) message: String,
    pub(crate) suppressed: bool,
}

/// `// qual:allow(...)` marker that matched no finding in its window.
#[derive(serde::Serialize)]
pub(crate) struct JsonOrphanSuppression {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) dimensions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonWildcardWarning {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) module_path: String,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonSummary {
    pub(crate) total: usize,
    pub(crate) integrations: usize,
    pub(crate) operations: usize,
    pub(crate) violations: usize,
    pub(crate) trivial: usize,
    pub(crate) suppressed: usize,
    pub(crate) all_suppressions: usize,
    pub(crate) iosp_score: f64,
    pub(crate) quality_score: f64,
    pub(crate) complexity_warnings: usize,
    pub(crate) magic_number_warnings: usize,
    pub(crate) coupling_warnings: usize,
    pub(crate) coupling_cycles: usize,
    pub(crate) duplicate_groups: usize,
    pub(crate) dead_code_warnings: usize,
    pub(crate) fragment_groups: usize,
    pub(crate) boilerplate_warnings: usize,
    pub(crate) srp_struct_warnings: usize,
    pub(crate) srp_module_warnings: usize,
    pub(crate) srp_param_warnings: usize,
    pub(crate) nesting_depth_warnings: usize,
    pub(crate) function_length_warnings: usize,
    pub(crate) unsafe_warnings: usize,
    pub(crate) error_handling_warnings: usize,
    pub(crate) wildcard_import_warnings: usize,
    pub(crate) sdp_violations: usize,
    pub(crate) tq_no_assertion_warnings: usize,
    pub(crate) tq_no_sut_warnings: usize,
    pub(crate) tq_untested_warnings: usize,
    pub(crate) tq_uncovered_warnings: usize,
    pub(crate) tq_untested_logic_warnings: usize,
    pub(crate) structural_srp_warnings: usize,
    pub(crate) structural_coupling_warnings: usize,
    pub(crate) repeated_match_groups: usize,
    pub(crate) suppression_ratio_exceeded: bool,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonCoupling {
    pub(crate) modules: Vec<JsonCouplingModule>,
    pub(crate) cycles: Vec<Vec<String>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) sdp_violations: Vec<JsonSdpViolation>,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonSdpViolation {
    pub(crate) from_module: String,
    pub(crate) to_module: String,
    pub(crate) from_instability: f64,
    pub(crate) to_instability: f64,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonCouplingModule {
    pub(crate) name: String,
    pub(crate) afferent: usize,
    pub(crate) efferent: usize,
    pub(crate) instability: f64,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonFunction {
    pub(crate) name: String,
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) parent_type: Option<String>,
    pub(crate) classification: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) severity: Option<Severity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) suppressed: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) logic: Vec<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) calls: Vec<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) complexity: Option<JsonComplexity>,
    pub(crate) parameter_count: usize,
    pub(crate) is_trait_impl: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) effort_score: Option<f64>,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonComplexity {
    pub(crate) logic_count: usize,
    pub(crate) call_count: usize,
    pub(crate) max_nesting: usize,
    pub(crate) cognitive_complexity: usize,
    pub(crate) cyclomatic_complexity: usize,
    pub(crate) function_lines: usize,
    pub(crate) unsafe_blocks: usize,
    pub(crate) unwrap_count: usize,
    pub(crate) expect_count: usize,
    pub(crate) panic_count: usize,
    pub(crate) todo_count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) hotspots: Vec<JsonHotspot>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) magic_numbers: Vec<JsonMagicNumber>,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonHotspot {
    pub(crate) line: usize,
    pub(crate) nesting_depth: usize,
    pub(crate) construct: String,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonMagicNumber {
    pub(crate) line: usize,
    pub(crate) value: String,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonDuplicateGroup {
    pub(crate) kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) similarity: Option<f64>,
    pub(crate) entries: Vec<JsonDuplicateEntry>,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonDuplicateEntry {
    pub(crate) name: String,
    pub(crate) qualified_name: String,
    pub(crate) file: String,
    pub(crate) line: usize,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonDeadCodeWarning {
    pub(crate) function_name: String,
    pub(crate) qualified_name: String,
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) kind: String,
    pub(crate) suggestion: String,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonFragmentGroup {
    pub(crate) statement_count: usize,
    pub(crate) entries: Vec<JsonFragmentEntry>,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonFragmentEntry {
    pub(crate) function_name: String,
    pub(crate) qualified_name: String,
    pub(crate) file: String,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonBoilerplateFind {
    pub(crate) pattern_id: String,
    pub(crate) file: String,
    pub(crate) line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) struct_name: Option<String>,
    pub(crate) description: String,
    pub(crate) suggestion: String,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonSrp {
    pub(crate) struct_warnings: Vec<JsonSrpWarning>,
    pub(crate) module_warnings: Vec<JsonModuleSrpWarning>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) param_warnings: Vec<JsonParamSrpWarning>,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonParamSrpWarning {
    pub(crate) function_name: String,
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) parameter_count: usize,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonSrpWarning {
    pub(crate) struct_name: String,
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) lcom4: usize,
    pub(crate) field_count: usize,
    pub(crate) method_count: usize,
    pub(crate) fan_out: usize,
    pub(crate) composite_score: f64,
    pub(crate) clusters: Vec<JsonSrpCluster>,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonSrpCluster {
    pub(crate) methods: Vec<String>,
    pub(crate) fields: Vec<String>,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonModuleSrpWarning {
    pub(crate) module: String,
    pub(crate) file: String,
    pub(crate) production_lines: usize,
    pub(crate) length_score: f64,
    pub(crate) independent_clusters: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) cluster_names: Vec<Vec<String>>,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonTqWarning {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) function_name: String,
    pub(crate) kind: String,
    pub(crate) suppressed: bool,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonStructuralWarning {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) name: String,
    pub(crate) code: String,
    pub(crate) dimension: String,
    pub(crate) detail: String,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonRepeatedMatchGroup {
    pub(crate) enum_name: String,
    pub(crate) entries: Vec<JsonRepeatedMatchEntry>,
}

#[derive(serde::Serialize)]
pub(crate) struct JsonRepeatedMatchEntry {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) function_name: String,
    pub(crate) arm_count: usize,
}
