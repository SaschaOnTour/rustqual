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
    let findings = crate::domain::AnalysisFindings {
        iosp: crate::app::projection::project_iosp(&results),
        ..Default::default()
    };
    AnalysisResult {
        results,
        summary,
        findings,
        data,
    }
}

#[test]
fn test_print_json_empty_yields_valid_json() {
    let analysis = make_analysis(vec![]);
    let json = crate::report::json::build_json_string(&analysis);
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON on empty input");
    assert_eq!(
        v["functions"].as_array().map(Vec::len),
        Some(0),
        "empty analysis produces empty functions array"
    );
}

#[test]
fn test_print_json_carries_violation_logic_and_call_locations() {
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
    let json = crate::report::json::build_json_string(&analysis);
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let f = v["functions"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"] == "bad_fn"))
        .expect("function `bad_fn`");
    assert_eq!(f["classification"], "violation");
    let logic = f["logic"].as_array().expect("logic array");
    assert_eq!(logic.len(), 1, "one logic location; got {json}");
    assert_eq!(logic[0]["kind"], "if");
    assert_eq!(logic[0]["line"], "1");
    let calls = f["calls"].as_array().expect("calls array");
    assert_eq!(calls.len(), 1, "one call location; got {json}");
    assert_eq!(calls[0]["name"], "f");
    assert_eq!(calls[0]["line"], "2");
}

#[test]
fn test_print_json_classifications_all_four() {
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
    let json = crate::report::json::build_json_string(&analysis);
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let by_name: std::collections::HashMap<String, String> = v["functions"]
        .as_array()
        .expect("functions array")
        .iter()
        .map(|f| {
            (
                f["name"].as_str().unwrap().into(),
                f["classification"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(by_name.get("a").map(String::as_str), Some("integration"));
    assert_eq!(by_name.get("b").map(String::as_str), Some("operation"));
    assert_eq!(by_name.get("c").map(String::as_str), Some("trivial"));
    assert_eq!(by_name.get("d").map(String::as_str), Some("violation"));
}

#[test]
fn test_print_json_carries_near_duplicate_similarity() {
    use crate::domain::findings::{
        DryFinding, DryFindingDetails, DryFindingKind, DuplicateParticipant,
    };
    use crate::domain::{Dimension, Finding, Severity};
    let participants = vec![
        DuplicateParticipant {
            function_name: "a".into(),
            file: "lib.rs".into(),
            line: 10,
        },
        DuplicateParticipant {
            function_name: "b".into(),
            file: "lib.rs".into(),
            line: 50,
        },
    ];
    let mut analysis = make_analysis(vec![]);
    analysis.findings.dry.push(DryFinding {
        common: Finding {
            file: "lib.rs".into(),
            line: 10,
            column: 0,
            dimension: Dimension::Dry,
            rule_id: "dry/duplicate/similar".into(),
            message: "near duplicate".into(),
            severity: Severity::Medium,
            suppressed: false,
        },
        kind: DryFindingKind::DuplicateSimilar,
        details: DryFindingDetails::Duplicate {
            participants,
            similarity: Some(0.91),
        },
    });
    let json = crate::report::json::build_json_string(&analysis);
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let group = v["duplicates"]
        .as_array()
        .and_then(|a| a.first())
        .expect("at least one duplicate group");
    let sim = group["similarity"].as_f64();
    assert_eq!(
        sim,
        Some(0.91),
        "NearDuplicate similarity must survive projection + reporter; got {json}"
    );
}

#[test]
fn test_print_json_carries_repeated_match_arm_count_and_distinct_groups() {
    use crate::domain::findings::{
        DryFinding, DryFindingDetails, DryFindingKind, RepeatedMatchParticipant,
    };
    use crate::domain::{Dimension, Finding, Severity};
    let common = |line: usize| Finding {
        file: "lib.rs".into(),
        line,
        column: 0,
        dimension: Dimension::Dry,
        rule_id: "dry/repeated_match".into(),
        message: "repeated match".into(),
        severity: Severity::Medium,
        suppressed: false,
    };
    let participant = |name: &str, line: usize, arms: usize| RepeatedMatchParticipant {
        function_name: name.into(),
        file: "lib.rs".into(),
        line,
        arm_count: arms,
    };
    let group_a = vec![participant("fa1", 10, 4), participant("fa2", 20, 4)];
    let group_b = vec![participant("fb1", 50, 3), participant("fb2", 60, 3)];
    let make_match = |line: usize, participants: Vec<RepeatedMatchParticipant>| DryFinding {
        common: common(line),
        kind: DryFindingKind::RepeatedMatch,
        details: DryFindingDetails::RepeatedMatch {
            enum_name: "MyEnum".into(),
            participants,
        },
    };
    let mut analysis = make_analysis(vec![]);
    analysis.findings.dry.push(make_match(10, group_a));
    analysis.findings.dry.push(make_match(50, group_b));
    let json = crate::report::json::build_json_string(&analysis);
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let groups = v["repeated_matches"]
        .as_array()
        .expect("repeated_matches array");
    assert_eq!(
        groups.len(),
        2,
        "two distinct groups over same enum must NOT collapse by enum_name; got {json}"
    );
    let arm_counts: Vec<u64> = groups
        .iter()
        .flat_map(|g| g["entries"].as_array().unwrap().iter())
        .map(|e| e["arm_count"].as_u64().unwrap_or(0))
        .collect();
    assert!(
        arm_counts.contains(&4) && arm_counts.contains(&3),
        "arm_count must survive projection (expected both 4 and 3); got {arm_counts:?} from {json}"
    );
}

#[test]
fn test_print_json_carries_srp_composite_score_clusters_length_score() {
    use crate::domain::findings::{
        ResponsibilityCluster, SrpFinding, SrpFindingDetails, SrpFindingKind,
    };
    use crate::domain::{Dimension, Finding, Severity};
    let common = |line: usize| Finding {
        file: "lib.rs".into(),
        line,
        column: 0,
        dimension: Dimension::Srp,
        rule_id: "srp/cohesion".into(),
        message: "low cohesion".into(),
        severity: Severity::Medium,
        suppressed: false,
    };
    let make_srp = |line: usize, kind: SrpFindingKind, details: SrpFindingDetails| SrpFinding {
        common: common(line),
        kind,
        details,
    };
    let mut analysis = make_analysis(vec![]);
    analysis.findings.srp.push(make_srp(
        10,
        SrpFindingKind::StructCohesion,
        SrpFindingDetails::StructCohesion {
            struct_name: "S".into(),
            lcom4: 3,
            field_count: 4,
            method_count: 6,
            fan_out: 2,
            composite_score: 0.85,
            clusters: vec![ResponsibilityCluster {
                methods: vec!["m1".into(), "m2".into()],
                fields: vec!["f1".into()],
            }],
        },
    ));
    analysis.findings.srp.push(make_srp(
        20,
        SrpFindingKind::ModuleLength,
        SrpFindingDetails::ModuleLength {
            module: "modA".into(),
            production_lines: 500,
            independent_clusters: 2,
            cluster_names: vec![vec!["m1".into(), "m2".into()], vec!["m3".into()]],
            length_score: 1.2,
        },
    ));
    let json = crate::report::json::build_json_string(&analysis);
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let s = v["srp"]["struct_warnings"]
        .as_array()
        .and_then(|a| a.first())
        .expect("struct_warnings present");
    assert_eq!(
        s["composite_score"].as_f64(),
        Some(0.85),
        "composite_score must survive; got {json}"
    );
    let clusters = s["clusters"].as_array().expect("clusters array");
    assert_eq!(clusters.len(), 1, "one cluster; got {json}");
    assert_eq!(
        clusters[0]["methods"].as_array().map(|a| a.len()),
        Some(2),
        "cluster methods preserved; got {json}"
    );
    let m = v["srp"]["module_warnings"]
        .as_array()
        .and_then(|a| a.first())
        .expect("module_warnings present");
    assert_eq!(
        m["length_score"].as_f64(),
        Some(1.2),
        "length_score must survive; got {json}"
    );
    let cluster_names = m["cluster_names"].as_array().expect("cluster_names array");
    assert_eq!(cluster_names.len(), 2, "two clusters; got {json}");
    assert_eq!(
        cluster_names[0].as_array().map(|a| a.len()),
        Some(2),
        "first cluster has 2 names; got {json}"
    );
}

#[test]
fn test_print_json_carries_complexity_counts() {
    // Build the JSON string, parse it, and assert the analyzer's
    // non-zero `logic_count` / `call_count` survive the projection +
    // reporter round-trip. Regression for v1.2.1: the typed-reporter
    // refactor dropped both fields from the projection, so every
    // function appeared as `logic_count: 0, call_count: 0` in JSON
    // until the round-12 fix added them back to
    // `ComplexityMetricsRecord` and the JSON projection.
    let mut func = make_result("counted", Classification::Operation);
    func.complexity = Some(ComplexityMetrics {
        logic_count: 5,
        call_count: 4,
        max_nesting: 1,
        ..Default::default()
    });
    let analysis = make_analysis(vec![func]);
    let json = crate::report::json::build_json_string(&analysis);
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let f = v["functions"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"] == "counted"))
        .expect("function `counted` present");
    let complexity = &f["complexity"];
    assert_eq!(
        complexity["logic_count"].as_u64(),
        Some(5),
        "logic_count must survive projection + reporter; got {json}"
    );
    assert_eq!(
        complexity["call_count"].as_u64(),
        Some(4),
        "call_count must survive projection + reporter; got {json}"
    );
}

#[test]
fn test_print_json_marks_suppressed_function() {
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
    let json = crate::report::json::build_json_string(&analysis);
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let f = v["functions"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"] == "suppressed"))
        .expect("function `suppressed`");
    assert_eq!(
        f["suppressed"].as_bool(),
        Some(true),
        "suppressed flag must propagate to JSON; got {json}"
    );
}

#[test]
fn test_print_json_carries_high_severity_for_many_violations() {
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
    let json = crate::report::json::build_json_string(&analysis);
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let f = v["functions"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"] == "complex"))
        .expect("function `complex`");
    let sev = f["severity"].as_str().unwrap_or("");
    assert!(
        matches!(sev, "high" | "medium"),
        "violations with 3+3 logic/call locations must map to medium/high severity; got `{sev}` in {json}"
    );
    assert_eq!(
        f["logic"].as_array().map(Vec::len),
        Some(3),
        "3 logic locations preserved"
    );
    assert_eq!(
        f["calls"].as_array().map(Vec::len),
        Some(3),
        "3 call locations preserved"
    );
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
