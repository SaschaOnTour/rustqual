use crate::adapters::analyzers::iosp::{
    compute_severity, CallOccurrence, Classification, ComplexityMetrics, FunctionAnalysis,
    LogicOccurrence,
};
use crate::report::text::*;
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

#[test]
fn test_print_report_empty_no_panic() {
    let results: Vec<FunctionAnalysis> = vec![];
    let summary = Summary::from_results(&results);
    print_summary_only(&summary, &[]);
}

#[test]
fn test_print_report_no_violations_no_panic() {
    let results = vec![make_result("good_fn", Classification::Integration)];
    let summary = Summary::from_results(&results);
    print_summary_only(&summary, &[]);
}

#[test]
fn test_print_report_with_violation_no_panic() {
    let results = vec![make_result(
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
    )];
    let summary = Summary::from_results(&results);
    print_summary_only(&summary, &[]);
}

#[test]
fn test_print_report_verbose_no_panic() {
    let results = vec![
        make_result("integrate_fn", Classification::Integration),
        make_result("operate_fn", Classification::Operation),
        make_result("trivial_fn", Classification::Trivial),
        make_result(
            "violate_fn",
            Classification::Violation {
                has_logic: true,
                has_own_calls: true,
                logic_locations: vec![LogicOccurrence {
                    kind: "for".into(),
                    line: 1,
                }],
                call_locations: vec![CallOccurrence {
                    name: "foo".into(),
                    line: 2,
                }],
            },
        ),
    ];
    let summary = Summary::from_results(&results);
    print_summary_only(&summary, &[]);
    print_files_only(&results);
}

#[test]
fn test_print_report_with_complexity_no_panic() {
    let mut func = make_result("complex_fn", Classification::Operation);
    func.complexity = Some(ComplexityMetrics {
        logic_count: 5,
        call_count: 0,
        max_nesting: 3,
        ..Default::default()
    });
    let results = vec![func];
    let summary = Summary::from_results(&results);
    print_summary_only(&summary, &[]);
    print_files_only(&results);
}

#[test]
fn test_print_report_suppressed_verbose_no_panic() {
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
    let results = vec![func];
    let summary = Summary::from_results(&results);
    print_summary_only(&summary, &[]);
    print_files_only(&results);
}
