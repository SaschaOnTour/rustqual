//! The JSON envelope: the top-level document, its summary, and the
//! dimension-independent entries. The per-dimension payload types live in
//! `dimensions` — one file of plain DTOs grew past the module-length limit,
//! and the seam between "the envelope" and "what a dimension puts in it" is
//! the one that was already there.

mod dimensions;

pub(crate) use dimensions::*;

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
    pub(crate) dead_types: Vec<JsonDeadTypeWarning>,
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

/// `// qual:allow(...)` marker reported as stale (matched no finding) or
/// too-loose (a metric pin too far above the value it covers). See `kind`.
/// `pub` (not `pub(crate)`) because it surfaces as `JsonReporter::OrphanView`
/// — a per-reporter view type on the public `ReporterImpl` trait.
#[derive(serde::Serialize)]
pub struct JsonOrphanSuppression {
    /// Which annotation is stale: `"allow"`, `"api"`, or `"test_helper"` —
    /// without it a consumer cannot tell a bare `qual:api` marker (no
    /// dimensions, no target) from a blanket `qual:allow`.
    pub(crate) marker: &'static str,
    pub(crate) file: String,
    pub(crate) line: usize,
    /// Why the marker is reported: `"stale"` (delete it) or `"too_loose"`
    /// (tighten the pin) — a stable token so CI consumers branch on the remedy.
    pub(crate) kind: &'static str,
    pub(crate) dimensions: Vec<String>,
    /// The targeted finding-kind (`"file_length=400"`, `"god_struct"`), absent
    /// for a blanket/invalid marker — so consumers see which target is being
    /// reported (stale or, for a metric pin, too-loose).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<String>,
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
    pub(crate) dead_type_warnings: usize,
    /// How `untested` was answered across the run: `"measured"` (a report
    /// answered every one), `"coverage-augmented"` (a report was read, some
    /// findings still came from the call graph) or `"call-graph-only"`. Each
    /// TQ warning carries its own `coverage` — this one describes the run.
    pub(crate) coverage: String,
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
    pub(crate) architecture_warnings: usize,
    pub(crate) orphan_suppressions: usize,
    pub(crate) dimension_scores: [f64; 7],
    pub(crate) suppression_ratio_exceeded: bool,
}
