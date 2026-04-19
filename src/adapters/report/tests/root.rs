use crate::adapters::analyzers::iosp::{
    compute_severity, CallOccurrence, Classification, ComplexityMetrics, FunctionAnalysis,
    LogicOccurrence,
};
use crate::report::*;

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
                        .map(|l| serde_json::json!({"kind": l.kind, "line": l.line.to_string()}))
                        .collect();
                    let calls: Vec<serde_json::Value> = call_locations
                        .iter()
                        .map(|c| serde_json::json!({"name": c.name, "line": c.line.to_string()}))
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
    assert!((summary.quality_score - 1.0).abs() < 1e-10);
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
    assert!((summary.quality_score - 1.0).abs() < 1e-10);
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
fn test_score_reflects_total_findings_realistically() {
    // 100 functions, 10 IOSP violations + 10 complexity warnings = 20 findings
    // With default weights (IOSP=0.25, CX=0.20), score should be significantly < 90%
    let mut summary = Summary {
        total: 100,
        violations: 10,
        iosp_score: 0.9, // 10/100 violations
        complexity_warnings: 10,
        ..Default::default()
    };
    summary.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
    assert!(
        summary.quality_score < 0.85,
        "20 findings / 100 functions should be < 85%, got {:.1}%",
        summary.quality_score * 100.0
    );
    assert!(
        summary.quality_score > 0.50,
        "20 findings / 100 functions should be > 50%, got {:.1}%",
        summary.quality_score * 100.0
    );
}

#[test]
fn test_score_100_percent_only_with_zero_findings() {
    // Any finding should prevent 100%
    let mut summary = Summary {
        total: 100,
        iosp_score: 1.0,
        magic_number_warnings: 1,
        ..Default::default()
    };
    summary.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
    assert!(
        summary.quality_score < 1.0,
        "1 finding should prevent 100%, got {:.1}%",
        summary.quality_score * 100.0
    );
}

#[test]
fn test_score_all_violations_is_near_zero() {
    // 100/100 IOSP violations → score should be very low, not 75%
    let mut summary = Summary {
        total: 100,
        violations: 100,
        iosp_score: 0.0, // 100% violations
        ..Default::default()
    };
    summary.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
    assert!(
        summary.quality_score < 0.10,
        "100% violations should give score < 10%, got {:.1}%",
        summary.quality_score * 100.0
    );
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
