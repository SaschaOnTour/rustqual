use crate::adapters::analyzers::iosp::{
    compute_severity, CallOccurrence, Classification, ComplexityMetrics, FunctionAnalysis,
    LogicOccurrence,
};
use crate::report::json::*;
use crate::report::{AnalysisResult, Summary};

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

fn make_analysis(results: Vec<FunctionAnalysis>) -> AnalysisResult {
    let summary = Summary::from_results(&results);
    let data = crate::app::projection::project_data(&results, None);
    AnalysisResult {
        results,
        summary,
        findings: crate::domain::AnalysisFindings::default(),
        data,
    }
}

#[test]
fn test_print_json_empty_no_panic() {
    let analysis = make_analysis(vec![]);
    print_json(&analysis);
}

#[test]
fn test_print_json_violation_no_panic() {
    let analysis = make_analysis(vec![make_result(
        "bad_fn",
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
    )]);
    print_json(&analysis);
}

#[test]
fn test_print_json_all_types_no_panic() {
    let analysis = make_analysis(vec![
        make_result("a", Classification::Integration),
        make_result("b", Classification::Operation),
        make_result("c", Classification::Trivial),
        make_result(
            "d",
            Classification::Violation {
                has_logic: true,
                has_own_calls: true,
                logic_locations: vec![LogicOccurrence {
                    kind: "match".into(),
                    line: 1,
                }],
                call_locations: vec![CallOccurrence {
                    name: "g".into(),
                    line: 2,
                }],
            },
        ),
    ]);
    print_json(&analysis);
}

#[test]
fn test_print_json_with_complexity_no_panic() {
    let mut func = make_result("f", Classification::Operation);
    func.complexity = Some(ComplexityMetrics {
        logic_count: 3,
        call_count: 0,
        max_nesting: 2,
        ..Default::default()
    });
    let analysis = make_analysis(vec![func]);
    print_json(&analysis);
}

#[test]
fn test_print_json_suppressed_no_panic() {
    let mut func = make_result(
        "suppressed",
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
    );
    func.suppressed = true;
    let analysis = make_analysis(vec![func]);
    print_json(&analysis);
}

#[test]
fn test_print_json_high_severity_no_panic() {
    let analysis = make_analysis(vec![make_result(
        "complex",
        Classification::Violation {
            has_logic: true,
            has_own_calls: true,
            logic_locations: vec![
                LogicOccurrence {
                    kind: "if".into(),
                    line: 1,
                },
                LogicOccurrence {
                    kind: "match".into(),
                    line: 2,
                },
                LogicOccurrence {
                    kind: "for".into(),
                    line: 3,
                },
            ],
            call_locations: vec![
                CallOccurrence {
                    name: "a".into(),
                    line: 4,
                },
                CallOccurrence {
                    name: "b".into(),
                    line: 5,
                },
                CallOccurrence {
                    name: "c".into(),
                    line: 6,
                },
            ],
        },
    )]);
    print_json(&analysis);
}

// ── JSON content tests (verifying fields are present) ──────

#[test]
fn test_json_summary_has_complexity_warnings_field() {
    let analysis = make_analysis(vec![make_result("f", Classification::Operation)]);
    let json = build_json_string(&analysis);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        parsed["summary"]["complexity_warnings"].is_number(),
        "JSON summary must include complexity_warnings field"
    );
}

#[test]
fn test_json_summary_has_magic_number_warnings_field() {
    let analysis = make_analysis(vec![make_result("f", Classification::Operation)]);
    let json = build_json_string(&analysis);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        parsed["summary"]["magic_number_warnings"].is_number(),
        "JSON summary must include magic_number_warnings field"
    );
}

#[test]
fn test_json_summary_has_all_dimension_fields() {
    let analysis = make_analysis(vec![make_result("f", Classification::Operation)]);
    let json = build_json_string(&analysis);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let s = &parsed["summary"];
    let expected_fields = [
        "total",
        "integrations",
        "operations",
        "violations",
        "trivial",
        "suppressed",
        "all_suppressions",
        "iosp_score",
        "quality_score",
        "complexity_warnings",
        "magic_number_warnings",
        "nesting_depth_warnings",
        "function_length_warnings",
        "unsafe_warnings",
        "error_handling_warnings",
        "coupling_warnings",
        "coupling_cycles",
        "duplicate_groups",
        "dead_code_warnings",
        "fragment_groups",
        "boilerplate_warnings",
        "srp_struct_warnings",
        "srp_module_warnings",
        "srp_param_warnings",
        "tq_no_assertion_warnings",
        "tq_no_sut_warnings",
        "tq_untested_warnings",
        "tq_uncovered_warnings",
        "tq_untested_logic_warnings",
        "suppression_ratio_exceeded",
    ];
    expected_fields.iter().for_each(|&field| {
        assert!(!s[field].is_null(), "JSON summary missing field: {field}");
    });
}

#[test]
fn test_json_complexity_has_extended_fields() {
    let mut func = make_result("f", Classification::Operation);
    func.complexity = Some(ComplexityMetrics {
        logic_count: 3,
        call_count: 1,
        max_nesting: 2,
        function_lines: 45,
        unsafe_blocks: 1,
        unwrap_count: 2,
        expect_count: 1,
        panic_count: 0,
        todo_count: 0,
        ..Default::default()
    });
    let analysis = make_analysis(vec![func]);
    let json = build_json_string(&analysis);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let c = &parsed["functions"][0]["complexity"];
    assert_eq!(c["function_lines"].as_u64().unwrap(), 45);
    assert_eq!(c["unsafe_blocks"].as_u64().unwrap(), 1);
    assert_eq!(c["unwrap_count"].as_u64().unwrap(), 2);
    assert_eq!(c["expect_count"].as_u64().unwrap(), 1);
    assert_eq!(c["panic_count"].as_u64().unwrap(), 0);
    assert_eq!(c["todo_count"].as_u64().unwrap(), 0);
}

#[test]
fn json_reporter_includes_orphan_suppressions_via_snapshot_view() {
    // Populate `findings.orphan_suppressions` ONLY (not the legacy
    // `analysis.orphan_suppressions` field) and verify the JSON
    // output still includes the orphans — proving the JSON reporter
    // reads them from the trait-driven `Snapshot::orphans` view.
    use crate::domain::findings::OrphanSuppression;
    let mut analysis = make_analysis(vec![]);
    analysis.findings.orphan_suppressions = vec![OrphanSuppression {
        file: "src/foo.rs".into(),
        line: 42,
        dimensions: vec![crate::findings::Dimension::Srp],
        reason: Some("legacy".into()),
    }];
    let json = build_json_string(&analysis);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let arr = parsed["orphan_suppressions"].as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["file"], "src/foo.rs");
    assert_eq!(arr[0]["line"], 42);
    assert_eq!(arr[0]["dimensions"][0], "srp");
    assert_eq!(arr[0]["reason"], "legacy");
}

#[test]
fn test_json_omits_empty_orphan_suppressions() {
    // When the list is empty (clean codebase), the field is elided
    // to keep JSON compact — matches the policy for other optional
    // arrays (duplicates, dead_code, etc.).
    let analysis = make_analysis(vec![]);
    let json = build_json_string(&analysis);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        parsed.get("orphan_suppressions").is_none(),
        "empty orphan list should be elided from JSON"
    );
}
