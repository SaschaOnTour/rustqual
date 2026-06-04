use super::*;

#[test]
fn build_srp_emit_dimension_specific_categories() {
    let cohesion = SrpFinding {
        common: Finding {
            file: "src/a.rs".into(),
            line: 10,
            column: 0,
            dimension: crate::findings::Dimension::Srp,
            rule_id: "srp/struct_cohesion".into(),
            message: "x".into(),
            severity: crate::domain::Severity::Medium,
            suppressed: false,
        },
        kind: SrpFindingKind::StructCohesion,
        details: SrpFindingDetails::StructCohesion {
            struct_name: "Foo".into(),
            lcom4: 4,
            field_count: 6,
            method_count: 8,
            fan_out: 3,
            composite_score: 0.0,
            clusters: vec![],
        },
    };
    let config = Config::default();
    let data = crate::domain::AnalysisData::default();
    let reporter = make_reporter(&config, &data);
    let rows = reporter.build_srp(&[cohesion]);
    let entries: Vec<Value> = rows
        .into_iter()
        .map(|r| format_srp_entry(r, &config))
        .collect();
    assert_eq!(entries[0]["category"], "srp_struct");
    assert!(entries[0]["detail"].as_str().unwrap().contains("LCOM4=4"));
}

#[test]
fn report_coupling_cycle_emits_arrow_chain() {
    let cycle = CouplingFinding {
        common: Finding {
            file: "".into(),
            line: 0,
            column: 0,
            dimension: crate::findings::Dimension::Coupling,
            rule_id: "coupling/cycle".into(),
            message: "x".into(),
            severity: crate::domain::Severity::Medium,
            suppressed: false,
        },
        kind: CouplingFindingKind::Cycle,
        details: CouplingFindingDetails::Cycle {
            modules: vec!["a".into(), "b".into(), "a".into()],
        },
    };
    let config = Config::default();
    let data = crate::domain::AnalysisData::default();
    let reporter = make_reporter(&config, &data);
    let rows = reporter.build_coupling(&[cycle]);
    let entries: Vec<Value> = rows.into_iter().map(format_coupling_entry).collect();
    assert_eq!(entries[0]["category"], "cycle");
    let detail = entries[0]["detail"].as_str().unwrap();
    assert!(detail.contains("a -> b -> a"), "got: {detail}");
}

#[test]
fn build_test_quality_emit_correct_categories() {
    let tq = TqFinding {
        common: Finding {
            file: "src/test.rs".into(),
            line: 1,
            column: 0,
            dimension: crate::findings::Dimension::TestQuality,
            rule_id: "tq/no_assertion".into(),
            message: "test fn has no asserts".into(),
            severity: crate::domain::Severity::Medium,
            suppressed: false,
        },
        kind: TqFindingKind::NoAssertion,
        function_name: "test_fn".into(),
        uncovered_lines: None,
    };
    let config = Config::default();
    let data = crate::domain::AnalysisData::default();
    let reporter = make_reporter(&config, &data);
    let rows = reporter.build_test_quality(&[tq]);
    let entries: Vec<Value> = rows.into_iter().map(format_tq_entry).collect();
    assert_eq!(entries[0]["category"], "no_assertion");
}

#[test]
fn report_architecture_severity_maps_independently() {
    let high = ArchitectureFinding {
        common: arch_common("src/foo.rs", 1, crate::domain::Severity::High),
    };
    let config = Config::default();
    let data = crate::domain::AnalysisData::default();
    let reporter = make_reporter(&config, &data);
    let rows = reporter.build_architecture(&[high]);
    let entries: Vec<Value> = rows.into_iter().map(format_arch_entry).collect();
    assert_eq!(entries[0]["category"], "architecture");
    assert!(entries[0]["detail"]
        .as_str()
        .unwrap()
        .contains("architecture/test"));
}

#[test]
fn empty_findings_produce_empty_chunks() {
    let config = Config::default();
    let data = crate::domain::AnalysisData::default();
    let reporter = make_reporter(&config, &data);
    assert!(reporter.build_iosp(&[]).is_empty());
    assert!(reporter.build_complexity(&[]).is_empty());
    assert!(reporter.build_dry(&[]).is_empty());
    assert!(reporter.build_srp(&[]).is_empty());
    assert!(reporter.build_coupling(&[]).is_empty());
    assert!(reporter.build_test_quality(&[]).is_empty());
    assert!(reporter.build_architecture(&[]).is_empty());
}
