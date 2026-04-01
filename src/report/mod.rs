// qual:allow(coupling) reason: "report naturally depends on all analysis modules"
mod baseline;
mod dot;
pub(crate) mod findings_list;
mod github;
mod html;
mod json;
mod json_srp;
mod json_structural;
mod json_tq;
mod json_types;
mod sarif;
mod suggestions;
mod text;

pub use baseline::{create_baseline, print_comparison};
pub use dot::print_dot;
pub use github::print_coupling_annotations;
pub use github::print_dry_annotations;
pub use github::print_github;
pub use github::print_srp_annotations;
pub use github::print_structural_annotations;
pub use github::print_tq_annotations;
// print_dry_section re-exported below
pub use html::print_html;
pub use json::print_json;
pub use sarif::print_sarif;
pub use suggestions::print_suggestions;
pub use text::print_coupling_section;
pub use text::print_dry_section;
pub use text::print_report;
pub use text::print_srp_section;
pub(crate) use text::print_structural_section;
pub(crate) use text::print_tq_section;

use crate::analyzer::{Classification, FunctionAnalysis};
use crate::dry::boilerplate::BoilerplateFind;
use crate::dry::dead_code::DeadCodeWarning;
use crate::dry::fragments::FragmentGroup;
use crate::dry::functions::DuplicateGroup;
use crate::dry::wildcards::WildcardImportWarning;

/// All outputs from a full analysis run.
pub struct AnalysisResult {
    pub results: Vec<FunctionAnalysis>,
    pub summary: Summary,
    pub coupling: Option<crate::coupling::CouplingAnalysis>,
    pub duplicates: Vec<DuplicateGroup>,
    pub dead_code: Vec<DeadCodeWarning>,
    pub fragments: Vec<FragmentGroup>,
    pub boilerplate: Vec<BoilerplateFind>,
    pub wildcard_warnings: Vec<WildcardImportWarning>,
    pub repeated_matches: Vec<crate::dry::match_patterns::RepeatedMatchGroup>,
    pub srp: Option<crate::srp::SrpAnalysis>,
    pub tq: Option<crate::tq::TqAnalysis>,
    pub structural: Option<crate::structural::StructuralAnalysis>,
}

/// Summary statistics for a full analysis run.
#[derive(Debug, Default)]
pub struct Summary {
    pub total: usize,
    pub integrations: usize,
    pub operations: usize,
    pub violations: usize,
    pub trivial: usize,
    pub suppressed: usize,
    /// IOSP compliance score (0.0 = all violations, 1.0 = fully compliant).
    /// Trivial and suppressed functions are excluded from the calculation.
    pub iosp_score: f64,
    /// Number of functions exceeding complexity thresholds.
    pub complexity_warnings: usize,
    /// Number of functions containing magic numbers.
    pub magic_number_warnings: usize,
    /// Number of functions exceeding nesting depth threshold.
    pub nesting_depth_warnings: usize,
    /// Number of functions exceeding function length threshold.
    pub function_length_warnings: usize,
    /// Number of functions containing unsafe blocks.
    pub unsafe_warnings: usize,
    /// Number of functions with error handling issues (unwrap/panic/todo).
    pub error_handling_warnings: usize,
    /// Number of modules with coupling warnings (exceeding thresholds).
    pub coupling_warnings: usize,
    /// Number of circular dependencies found.
    pub coupling_cycles: usize,
    /// Number of duplicate function groups found.
    pub duplicate_groups: usize,
    /// Number of dead code warnings.
    pub dead_code_warnings: usize,
    /// Number of duplicate fragment groups found.
    pub fragment_groups: usize,
    /// Number of boilerplate pattern findings.
    pub boilerplate_warnings: usize,
    /// Number of structs exceeding SRP smell threshold.
    pub srp_struct_warnings: usize,
    /// Number of modules exceeding production line thresholds.
    pub srp_module_warnings: usize,
    /// Number of functions with `#[allow(clippy::too_many_arguments)]`.
    pub srp_param_warnings: usize,
    /// Number of wildcard import warnings.
    pub wildcard_import_warnings: usize,
    /// Number of repeated match pattern groups (DRY-005).
    pub repeated_match_groups: usize,
    /// Number of Stable Dependencies Principle violations.
    pub sdp_violations: usize,
    /// Number of TQ-001 warnings: tests without assertions.
    pub tq_no_assertion_warnings: usize,
    /// Number of TQ-002 warnings: tests without SUT calls.
    pub tq_no_sut_warnings: usize,
    /// Number of TQ-003 warnings: untested production functions.
    pub tq_untested_warnings: usize,
    /// Number of TQ-004 warnings: uncovered production functions (LCOV).
    pub tq_uncovered_warnings: usize,
    /// Number of TQ-005 warnings: untested logic branches (LCOV).
    pub tq_untested_logic_warnings: usize,
    /// Number of structural binary check warnings mapped to SRP.
    pub structural_srp_warnings: usize,
    /// Number of structural binary check warnings mapped to Coupling.
    pub structural_coupling_warnings: usize,
    /// Weighted quality score across all dimensions (0.0–1.0).
    pub quality_score: f64,
    /// Per-dimension scores: [IOSP, Complexity, DRY, SRP, Coupling, Test].
    pub dimension_scores: [f64; 6],
    /// Total number of ALL allow suppressions: `// qual:allow` + `#[allow(...)]`.
    pub all_suppressions: usize,
    /// Whether the suppression ratio exceeds the configured maximum.
    pub suppression_ratio_exceeded: bool,
}

impl Summary {
    pub fn from_results(results: &[FunctionAnalysis]) -> Self {
        let mut s = Self {
            total: results.len(),
            ..Default::default()
        };
        for r in results {
            if r.suppressed {
                s.suppressed += 1;
                continue;
            }
            match &r.classification {
                Classification::Integration => s.integrations += 1,
                Classification::Operation => s.operations += 1,
                Classification::Violation { .. } => s.violations += 1,
                Classification::Trivial => s.trivial += 1,
            }
        }
        // Score: ratio of compliant non-trivial functions
        let non_trivial = s.integrations + s.operations + s.violations;
        s.iosp_score = if non_trivial > 0 {
            (s.integrations + s.operations) as f64 / non_trivial as f64
        } else {
            1.0
        };
        s
    }

    /// Compute the overall quality score from all dimension findings.
    /// Operation: arithmetic logic on summary fields.
    pub fn compute_quality_score(&mut self, weights: &[f64; 6]) {
        let n = self.total.max(1) as f64;
        let complexity_count = self.complexity_warnings
            + self.magic_number_warnings
            + self.nesting_depth_warnings
            + self.function_length_warnings
            + self.unsafe_warnings
            + self.error_handling_warnings;
        let tq_count = self.tq_no_assertion_warnings
            + self.tq_no_sut_warnings
            + self.tq_untested_warnings
            + self.tq_uncovered_warnings
            + self.tq_untested_logic_warnings;
        self.dimension_scores = [
            self.iosp_score,
            1.0 - (complexity_count as f64 / n).min(1.0),
            1.0 - ((self.duplicate_groups
                + self.fragment_groups
                + self.dead_code_warnings
                + self.boilerplate_warnings
                + self.wildcard_import_warnings
                + self.repeated_match_groups) as f64
                / n)
                .min(1.0),
            1.0 - ((self.srp_struct_warnings + self.srp_module_warnings + self.srp_param_warnings
                + self.structural_srp_warnings) as f64
                / n)
                .min(1.0),
            1.0 - ((self.coupling_warnings + self.coupling_cycles * 2 + self.sdp_violations
                + self.structural_coupling_warnings) as f64
                / n)
                .min(1.0),
            1.0 - (tq_count as f64 / n).min(1.0),
        ];
        self.quality_score = self
            .dimension_scores
            .iter()
            .zip(weights.iter())
            .map(|(s, w)| s * w)
            .sum();
    }

    /// Total number of findings across all dimensions.
    /// Operation: arithmetic.
    pub fn total_findings(&self) -> usize {
        self.violations
            + self.complexity_warnings
            + self.magic_number_warnings
            + self.nesting_depth_warnings
            + self.function_length_warnings
            + self.unsafe_warnings
            + self.error_handling_warnings
            + self.duplicate_groups
            + self.fragment_groups
            + self.dead_code_warnings
            + self.boilerplate_warnings
            + self.srp_struct_warnings
            + self.srp_module_warnings
            + self.srp_param_warnings
            + self.wildcard_import_warnings
            + self.repeated_match_groups
            + self.coupling_warnings
            + self.coupling_cycles
            + self.sdp_violations
            + self.tq_no_assertion_warnings
            + self.tq_no_sut_warnings
            + self.tq_untested_warnings
            + self.tq_uncovered_warnings
            + self.tq_untested_logic_warnings
            + self.structural_srp_warnings
            + self.structural_coupling_warnings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::{
        compute_severity, CallOccurrence, Classification, ComplexityMetrics, FunctionAnalysis,
        LogicOccurrence,
    };

    fn make_result(name: &str, classification: Classification) -> FunctionAnalysis {
        let severity = compute_severity(&classification);
        FunctionAnalysis {
            name: name.to_string(),
            file: "test.rs".to_string(),
            line: 1,
            classification,
            parent_type: None,
            suppressed: false,
            complexity: None,
            qualified_name: name.to_string(),
            severity,
            cognitive_warning: false,
            cyclomatic_warning: false,
            nesting_depth_warning: false,
            function_length_warning: false,
            unsafe_warning: false,
            error_handling_warning: false,
            complexity_suppressed: false,
            own_calls: vec![],
            parameter_count: 0,
            is_trait_impl: false,
            is_test: false,
            effort_score: None,
        }
    }

    #[test]
    fn test_summary_counts() {
        let results = vec![
            make_result("integrate_a", Classification::Integration),
            make_result("integrate_b", Classification::Integration),
            make_result("operate", Classification::Operation),
            make_result(
                "violate",
                Classification::Violation {
                    has_logic: true,
                    has_own_calls: true,
                    logic_locations: vec![LogicOccurrence {
                        kind: "if".into(),
                        line: 5,
                    }],
                    call_locations: vec![CallOccurrence {
                        name: "foo".into(),
                        line: 6,
                    }],
                },
            ),
            make_result("trivial_fn", Classification::Trivial),
        ];
        let summary = Summary::from_results(&results);
        assert_eq!(summary.total, 5);
        assert_eq!(summary.integrations, 2);
        assert_eq!(summary.operations, 1);
        assert_eq!(summary.violations, 1);
        assert_eq!(summary.trivial, 1);
    }

    #[test]
    fn test_summary_empty() {
        let results: Vec<FunctionAnalysis> = vec![];
        let summary = Summary::from_results(&results);
        assert_eq!(summary.total, 0);
        assert_eq!(summary.integrations, 0);
        assert_eq!(summary.operations, 0);
        assert_eq!(summary.violations, 0);
        assert_eq!(summary.trivial, 0);
    }

    #[test]
    fn test_suppressed_not_counted_as_violation() {
        let mut func = make_result(
            "suppressed_fn",
            Classification::Violation {
                has_logic: true,
                has_own_calls: true,
                logic_locations: vec![LogicOccurrence {
                    kind: "if".into(),
                    line: 1,
                }],
                call_locations: vec![CallOccurrence {
                    name: "foo".into(),
                    line: 2,
                }],
            },
        );
        func.suppressed = true;
        let results = vec![func];
        let summary = Summary::from_results(&results);
        assert_eq!(summary.violations, 0);
        assert_eq!(summary.suppressed, 1);
    }

    #[test]
    fn test_json_structure() {
        let results = vec![make_result("my_func", Classification::Integration)];
        let summary = Summary::from_results(&results);

        let json_value = serde_json::json!({
            "summary": {
                "total": summary.total,
                "integrations": summary.integrations,
                "operations": summary.operations,
                "violations": summary.violations,
                "trivial": summary.trivial,
            },
            "functions": [
                {
                    "name": "my_func",
                    "file": "test.rs",
                    "line": 1,
                    "parent_type": null,
                    "classification": "integration",
                }
            ]
        });

        assert!(
            json_value.get("summary").is_some(),
            "JSON must have a 'summary' key"
        );
        assert!(
            json_value.get("functions").is_some(),
            "JSON must have a 'functions' key"
        );

        let funcs = json_value["functions"].as_array().unwrap();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0]["classification"], "integration");
    }

    #[test]
    fn test_json_violation_has_logic_and_calls() {
        let results = vec![make_result(
            "bad_fn",
            Classification::Violation {
                has_logic: true,
                has_own_calls: true,
                logic_locations: vec![
                    LogicOccurrence {
                        kind: "if".into(),
                        line: 3,
                    },
                    LogicOccurrence {
                        kind: "match".into(),
                        line: 7,
                    },
                ],
                call_locations: vec![CallOccurrence {
                    name: "helper".into(),
                    line: 5,
                }],
            },
        )];
        let summary = Summary::from_results(&results);

        let json_functions: Vec<serde_json::Value> = results
            .iter()
            .map(|f| {
                let (classification, logic, calls) = match &f.classification {
                    Classification::Violation {
                        logic_locations,
                        call_locations,
                        ..
                    } => {
                        let logic: Vec<serde_json::Value> = logic_locations
                            .iter()
                            .map(
                                |l| serde_json::json!({"kind": l.kind, "line": l.line.to_string()}),
                            )
                            .collect();
                        let calls: Vec<serde_json::Value> = call_locations
                            .iter()
                            .map(
                                |c| serde_json::json!({"name": c.name, "line": c.line.to_string()}),
                            )
                            .collect();
                        ("violation", logic, calls)
                    }
                    _ => unreachable!(),
                };
                serde_json::json!({
                    "name": f.name,
                    "file": f.file,
                    "line": f.line,
                    "parent_type": f.parent_type,
                    "classification": classification,
                    "logic": logic,
                    "calls": calls,
                })
            })
            .collect();

        let output = serde_json::json!({
            "summary": {
                "total": summary.total,
                "integrations": summary.integrations,
                "operations": summary.operations,
                "violations": summary.violations,
                "trivial": summary.trivial,
            },
            "functions": json_functions,
        });

        let func = &output["functions"][0];
        assert_eq!(func["classification"], "violation");

        let logic = func["logic"].as_array().unwrap();
        assert_eq!(logic.len(), 2);
        assert_eq!(logic[0]["kind"], "if");
        assert_eq!(logic[1]["kind"], "match");

        let calls = func["calls"].as_array().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["name"], "helper");
    }

    #[test]
    fn test_json_integration_no_logic() {
        let results = vec![make_result("orchestrator", Classification::Integration)];
        let summary = Summary::from_results(&results);

        let json_value = serde_json::json!({
            "summary": {
                "total": summary.total,
                "integrations": summary.integrations,
                "operations": summary.operations,
                "violations": summary.violations,
                "trivial": summary.trivial,
            },
            "functions": [
                {
                    "name": "orchestrator",
                    "file": "test.rs",
                    "line": 1,
                    "parent_type": null,
                    "classification": "integration",
                }
            ]
        });

        let func = &json_value["functions"][0];
        assert!(
            func.get("logic").is_none(),
            "Integration should not have logic array"
        );
        assert!(
            func.get("calls").is_none(),
            "Integration should not have calls array"
        );
    }

    #[test]
    fn test_summary_total_matches() {
        let results = vec![
            make_result("a", Classification::Integration),
            make_result("b", Classification::Operation),
            make_result("c", Classification::Trivial),
            make_result(
                "d",
                Classification::Violation {
                    has_logic: true,
                    has_own_calls: true,
                    logic_locations: vec![],
                    call_locations: vec![],
                },
            ),
        ];
        let summary = Summary::from_results(&results);
        assert_eq!(summary.total, results.len());
    }

    #[test]
    fn test_baseline_roundtrip() {
        let results = vec![
            make_result("good_fn", Classification::Integration),
            make_result(
                "bad_fn",
                Classification::Violation {
                    has_logic: true,
                    has_own_calls: true,
                    logic_locations: vec![LogicOccurrence {
                        kind: "if".into(),
                        line: 1,
                    }],
                    call_locations: vec![CallOccurrence {
                        name: "helper".into(),
                        line: 2,
                    }],
                },
            ),
        ];
        let summary = Summary::from_results(&results);
        let baseline_json = create_baseline(&results, &summary);

        let parsed: serde_json::Value = serde_json::from_str(&baseline_json).unwrap();
        assert!(parsed["iosp_score"].as_f64().is_some());
        assert_eq!(parsed["violations"].as_u64().unwrap(), 1);
    }

    #[test]
    fn test_quality_score_perfect() {
        let results = vec![
            make_result("a", Classification::Integration),
            make_result("b", Classification::Operation),
        ];
        let mut summary = Summary::from_results(&results);
        summary.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
        assert!((summary.quality_score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_quality_score_with_violations() {
        let results = vec![
            make_result("a", Classification::Integration),
            make_result(
                "b",
                Classification::Violation {
                    has_logic: true,
                    has_own_calls: true,
                    logic_locations: vec![LogicOccurrence {
                        kind: "if".into(),
                        line: 1,
                    }],
                    call_locations: vec![CallOccurrence {
                        name: "f".into(),
                        line: 2,
                    }],
                },
            ),
        ];
        let mut summary = Summary::from_results(&results);
        summary.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
        assert!(summary.quality_score < 1.0);
        assert!(summary.quality_score > 0.0);
    }

    #[test]
    fn test_quality_score_empty() {
        let results: Vec<FunctionAnalysis> = vec![];
        let mut summary = Summary::from_results(&results);
        summary.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
        assert!((summary.quality_score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_quality_score_with_warnings() {
        let results = vec![
            make_result("a", Classification::Integration),
            make_result("b", Classification::Operation),
            make_result("c", Classification::Operation),
            make_result("d", Classification::Operation),
        ];
        let mut summary = Summary::from_results(&results);
        summary.complexity_warnings = 2;
        summary.duplicate_groups = 1;
        summary.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
        assert!(summary.quality_score < 1.0);
        assert!(summary.dimension_scores[1] < 1.0); // complexity
        assert!(summary.dimension_scores[2] < 1.0); // DRY
    }

    #[test]
    fn test_total_findings() {
        let summary = Summary {
            violations: 1,
            complexity_warnings: 2,
            magic_number_warnings: 1,
            duplicate_groups: 1,
            coupling_cycles: 1,
            ..Summary::default()
        };
        assert_eq!(summary.total_findings(), 6);
    }

    #[test]
    fn test_complexity_in_function_analysis() {
        let func = FunctionAnalysis {
            name: "f".to_string(),
            file: "test.rs".to_string(),
            line: 1,
            classification: Classification::Operation,
            parent_type: None,
            suppressed: false,
            complexity: Some(ComplexityMetrics {
                logic_count: 3,
                call_count: 0,
                max_nesting: 2,
                ..Default::default()
            }),
            qualified_name: "f".to_string(),
            severity: None,
            cognitive_warning: false,
            cyclomatic_warning: false,
            nesting_depth_warning: false,
            function_length_warning: false,
            unsafe_warning: false,
            error_handling_warning: false,
            complexity_suppressed: false,
            own_calls: vec![],
            parameter_count: 0,
            is_trait_impl: false,
            is_test: false,
            effort_score: None,
        };
        assert_eq!(func.complexity.as_ref().unwrap().logic_count, 3);
        assert_eq!(func.complexity.as_ref().unwrap().max_nesting, 2);
    }

    #[test]
    fn test_suppression_ratio_default_false() {
        let summary = Summary::default();
        assert!(!summary.suppression_ratio_exceeded);
    }

    #[test]
    fn test_suppression_ratio_flag_preserved() {
        let summary = Summary {
            suppression_ratio_exceeded: true,
            ..Summary::default()
        };
        assert!(summary.suppression_ratio_exceeded);
    }
}
