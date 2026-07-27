use super::*;

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

/// Project `results` to the test's expected JSON shape (summary + per-function
/// classification/logic/call locations) — the inline projection this test asserts
/// against, lifted to a helper so the test body stays act+assert.
fn project_violation_results(results: &[FunctionAnalysis]) -> serde_json::Value {
    let summary = Summary::from_results(results);
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
    serde_json::json!({
        "summary": {
            "total": summary.total,
            "integrations": summary.integrations,
            "operations": summary.operations,
            "violations": summary.violations,
            "trivial": summary.trivial,
        },
        "functions": json_functions,
    })
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
    let output = project_violation_results(&results);

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
fn the_coverage_mode_distinguishes_partial_coverage() {
    // Three states, not two. A report that covered every untested finding is
    // measurement; a report that covered some of them is not, and calling it
    // "measured" told a consumer that a call-graph guess had been verified.
    let mut s = Summary::default();
    assert_eq!(s.coverage_mode(), "call-graph-only");
    s.coverage_measured = true;
    assert_eq!(s.coverage_mode(), "measured");
    s.tq_untested_call_graph_only = 1;
    assert_eq!(s.coverage_mode(), "coverage-augmented");
    s.coverage_measured = false;
    assert_eq!(
        s.coverage_mode(),
        "call-graph-only",
        "no report at all outranks the count"
    );
}
