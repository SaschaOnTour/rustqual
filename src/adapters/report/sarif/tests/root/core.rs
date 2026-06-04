use super::*;

#[test]
fn test_print_sarif_emits_no_results_when_clean() {
    let analysis = make_analysis(vec![make_result("good_fn", Classification::Integration)]);
    let s = build_sarif_string(&analysis);
    let v: serde_json::Value = serde_json::from_str(&s).expect("valid SARIF JSON");
    let results = &v["runs"][0]["results"];
    assert_eq!(
        results.as_array().map(Vec::len),
        Some(0),
        "Integration function should produce no SARIF results; got {s}"
    );
}

#[test]
fn test_print_sarif_emits_violation_with_location() {
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
    let s = build_sarif_string(&analysis);
    let v: serde_json::Value = serde_json::from_str(&s).expect("valid SARIF JSON");
    let results = v["runs"][0]["results"].as_array().expect("results array");
    assert!(
        !results.is_empty(),
        "violation must produce SARIF result; got {s}"
    );
    let r = &results[0];
    let physical = &r["locations"][0]["physicalLocation"];
    assert_eq!(physical["artifactLocation"]["uri"], "test.rs");
    assert_eq!(physical["region"]["startLine"], 1);
    let rule_id = r["ruleId"].as_str().unwrap_or("");
    assert!(!rule_id.is_empty(), "rule_id must be set; got {s}");
}

#[test]
fn test_print_sarif_severity_for_many_violations_is_error_or_warning() {
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
    let s = build_sarif_string(&analysis);
    let v: serde_json::Value = serde_json::from_str(&s).expect("valid SARIF JSON");
    let level = v["runs"][0]["results"][0]["level"].as_str().unwrap_or("");
    assert!(
        matches!(level, "warning" | "error"),
        "3+3 violation must map to warning or error level; got `{level}` in {s}"
    );
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
