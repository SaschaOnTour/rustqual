use super::*;

#[test]
fn build_iosp_emits_violation_with_logic_and_call_lines() {
    use crate::domain::findings::{CallLocation, LogicLocation};
    let f = IospFinding {
        common: Finding {
            file: "src/lib.rs".into(),
            line: 40,
            column: 0,
            dimension: crate::findings::Dimension::Iosp,
            rule_id: "iosp/violation".into(),
            message: "ignored".into(),
            severity: crate::domain::Severity::Medium,
            suppressed: false,
        },
        logic_locations: vec![
            LogicLocation {
                kind: "if".into(),
                line: 44,
            },
            LogicLocation {
                kind: "for".into(),
                line: 47,
            },
        ],
        call_locations: vec![CallLocation {
            name: "helper".into(),
            line: 50,
        }],
        effort_score: None,
    };
    let config = Config::default();
    let data = crate::domain::AnalysisData::default();
    let reporter = make_reporter(&config, &data);
    let rows = reporter.build_iosp(&[f]);
    assert_eq!(rows.len(), 1);
    let entries: Vec<Value> = rows.into_iter().map(format_iosp_entry).collect();
    let detail = entries[0]["detail"].as_str().unwrap();
    assert!(detail.contains("logic lines 44,47"), "got: {detail}");
    assert!(detail.contains("call lines 50"), "got: {detail}");
    assert_eq!(entries[0]["category"], "violation");
}

#[test]
fn build_iosp_resolves_function_name_via_data() {
    let mut data = crate::domain::AnalysisData::default();
    data.functions.push(violation_record("src/lib.rs", 40));
    let f = iosp_finding("src/lib.rs", 40);
    let config = Config::default();
    let reporter = make_reporter(&config, &data);
    let rows = reporter.build_iosp(&[f]);
    let entries: Vec<Value> = rows.into_iter().map(format_iosp_entry).collect();
    assert_eq!(entries[0]["fn"], "MyType::bad_fn");
}

#[test]
fn report_complexity_threshold_findings_include_max() {
    let f = ComplexityFinding {
        common: Finding {
            file: "src/lib.rs".into(),
            line: 1,
            column: 0,
            dimension: crate::findings::Dimension::Complexity,
            rule_id: "complexity/cognitive".into(),
            message: "x".into(),
            severity: crate::domain::Severity::Medium,
            suppressed: false,
        },
        kind: ComplexityFindingKind::Cognitive,
        metric_value: 25,
        threshold: 10,
        hotspot: None,
    };
    let config = Config::default();
    let data = crate::domain::AnalysisData::default();
    let reporter = make_reporter(&config, &data);
    let rows = reporter.build_complexity(&[f]);
    let entries: Vec<Value> = rows.into_iter().map(format_complexity_entry).collect();
    assert_eq!(entries[0]["category"], "cognitive_complexity");
    let detail = entries[0]["detail"].as_str().unwrap();
    assert!(detail.contains("25"), "got: {detail}");
    assert!(detail.contains("max 10"), "got: {detail}");
}

#[test]
fn report_dry_duplicate_includes_partner_locations() {
    let f = DryFinding {
        common: Finding {
            file: "src/a.rs".into(),
            line: 10,
            column: 0,
            dimension: crate::findings::Dimension::Dry,
            rule_id: "dry/duplicate/exact".into(),
            message: "x".into(),
            severity: crate::domain::Severity::Medium,
            suppressed: false,
        },
        kind: DryFindingKind::DuplicateExact,
        details: DryFindingDetails::Duplicate {
            participants: vec![
                DuplicateParticipant {
                    function_name: "fn_a".into(),
                    file: "src/a.rs".into(),
                    line: 10,
                },
                DuplicateParticipant {
                    function_name: "fn_b".into(),
                    file: "src/b.rs".into(),
                    line: 20,
                },
            ],
            similarity: None,
        },
    };
    let config = Config::default();
    let data = crate::domain::AnalysisData::default();
    let reporter = make_reporter(&config, &data);
    let rows = reporter.build_dry(&[f]);
    let entries: Vec<Value> = rows.into_iter().map(format_dry_entry).collect();
    assert_eq!(entries[0]["category"], "duplicate");
    let detail = entries[0]["detail"].as_str().unwrap();
    assert!(detail.contains("src/b.rs:20"), "got: {detail}");
    assert!(
        !detail.contains("src/a.rs:10"),
        "self-link excluded; got: {detail}"
    );
}

#[test]
fn report_dry_dead_code_uses_suggestion() {
    let f = DryFinding {
        common: Finding {
            file: "src/foo.rs".into(),
            line: 5,
            column: 0,
            dimension: crate::findings::Dimension::Dry,
            rule_id: "dry/dead_code/uncalled".into(),
            message: "x".into(),
            severity: crate::domain::Severity::Medium,
            suppressed: false,
        },
        kind: DryFindingKind::DeadCodeUncalled,
        details: DryFindingDetails::DeadCode {
            qualified_name: "module::dead_fn".into(),
            suggestion: Some("remove".into()),
        },
    };
    let config = Config::default();
    let data = crate::domain::AnalysisData::default();
    let reporter = make_reporter(&config, &data);
    let rows = reporter.build_dry(&[f]);
    let entries: Vec<Value> = rows.into_iter().map(format_dry_entry).collect();
    let detail = entries[0]["detail"].as_str().unwrap();
    assert!(detail.contains("module::dead_fn"));
    assert!(detail.contains("remove"));
}
