use crate::adapters::analyzers::iosp::{
    compute_severity, CallOccurrence, Classification, FunctionAnalysis, LogicOccurrence,
};
use crate::report::sarif::*;
use crate::report::Summary;

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
    AnalysisResult {
        results,
        summary,
        coupling: None,
        duplicates: vec![],
        dead_code: vec![],
        fragments: vec![],
        boilerplate: vec![],
        wildcard_warnings: vec![],
        repeated_matches: vec![],
        srp: None,
        tq: None,
        structural: None,
        architecture_findings: vec![],
    }
}

#[test]
fn test_print_sarif_no_violations_no_panic() {
    let analysis = make_analysis(vec![make_result("good_fn", Classification::Integration)]);
    print_sarif(&analysis);
}

#[test]
fn test_print_sarif_with_violation_no_panic() {
    let analysis = make_analysis(vec![make_result(
        "bad_fn",
        Classification::Violation {
            has_logic: true,
            has_own_calls: true,
            logic_locations: vec![LogicOccurrence {
                kind: "if".into(),
                line: 5,
            }],
            call_locations: vec![CallOccurrence {
                name: "helper".into(),
                line: 6,
            }],
        },
    )]);
    print_sarif(&analysis);
}

#[test]
fn test_print_sarif_high_severity_no_panic() {
    let analysis = make_analysis(vec![make_result(
        "complex_fn",
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
    print_sarif(&analysis);
}

#[test]
fn test_print_sarif_suppressed_skipped() {
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
                name: "f".into(),
                line: 2,
            }],
        },
    );
    func.suppressed = true;
    let analysis = make_analysis(vec![func]);
    print_sarif(&analysis);
}

#[test]
fn test_print_sarif_multiple_violations() {
    let analysis = make_analysis(vec![
        make_result(
            "bad1",
            Classification::Violation {
                has_logic: true,
                has_own_calls: true,
                logic_locations: vec![LogicOccurrence {
                    kind: "if".into(),
                    line: 1,
                }],
                call_locations: vec![CallOccurrence {
                    name: "a".into(),
                    line: 2,
                }],
            },
        ),
        make_result(
            "bad2",
            Classification::Violation {
                has_logic: true,
                has_own_calls: true,
                logic_locations: vec![LogicOccurrence {
                    kind: "while".into(),
                    line: 10,
                }],
                call_locations: vec![CallOccurrence {
                    name: "b".into(),
                    line: 12,
                }],
            },
        ),
    ]);
    print_sarif(&analysis);
}
