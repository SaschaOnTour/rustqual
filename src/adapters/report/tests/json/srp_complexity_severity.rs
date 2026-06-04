use super::*;

/// Analysis with one struct-cohesion + one module-length SRP finding carrying
/// composite-score / clusters / length-score detail (the JSON-projection arrange).
fn srp_composite_analysis() -> AnalysisResult {
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
    analysis
}

#[test]
fn test_print_json_carries_srp_composite_score_clusters_length_score() {
    let v = json_value(&srp_composite_analysis());
    let s = v["srp"]["struct_warnings"]
        .as_array()
        .and_then(|a| a.first())
        .expect("struct_warnings present");
    assert_eq!(
        s["composite_score"].as_f64(),
        Some(0.85),
        "composite_score must survive; got {v}"
    );
    let clusters = s["clusters"].as_array().expect("clusters array");
    assert_eq!(clusters.len(), 1, "one cluster; got {v}");
    assert_eq!(
        clusters[0]["methods"].as_array().map(|a| a.len()),
        Some(2),
        "cluster methods preserved; got {v}"
    );
    let m = v["srp"]["module_warnings"]
        .as_array()
        .and_then(|a| a.first())
        .expect("module_warnings present");
    assert_eq!(
        m["length_score"].as_f64(),
        Some(1.2),
        "length_score must survive; got {v}"
    );
    let cluster_names = m["cluster_names"].as_array().expect("cluster_names array");
    assert_eq!(cluster_names.len(), 2, "two clusters; got {v}");
    assert_eq!(
        cluster_names[0].as_array().map(|a| a.len()),
        Some(2),
        "first cluster has 2 names; got {v}"
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
    let v = json_value(&analysis);
    let complexity = &function_named(&v, "counted")["complexity"];
    assert_eq!(
        complexity["logic_count"].as_u64(),
        Some(5),
        "logic_count must survive projection + reporter; got {v}"
    );
    assert_eq!(
        complexity["call_count"].as_u64(),
        Some(4),
        "call_count must survive projection + reporter; got {v}"
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
    let v = json_value(&analysis);
    let f = function_named(&v, "suppressed");
    assert_eq!(
        f["suppressed"].as_bool(),
        Some(true),
        "suppressed flag must propagate to JSON; got {v}"
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
    let v = json_value(&analysis);
    let f = function_named(&v, "complex");
    let sev = f["severity"].as_str().unwrap_or("");
    assert!(
        matches!(sev, "high" | "medium"),
        "violations with 3+3 logic/call locations must map to medium/high severity; got `{sev}` in {v}"
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
