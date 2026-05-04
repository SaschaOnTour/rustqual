//! AI reporter tests.
//!
//! Tests focus on `build_ai_value` (the public entry) plus per-method
//! AiReporter trait coverage. Loose content style: assert key fragments
//! and structural shape, not exact wording.

use crate::adapters::analyzers::iosp::{
    CallOccurrence, Classification, ComplexityMetrics, FunctionAnalysis, LogicOccurrence,
    MagicNumberOccurrence,
};
use crate::config::Config;
use crate::domain::analysis_data::FunctionRecord;
use crate::domain::findings::{
    ArchitectureFinding, ComplexityFinding, ComplexityFindingKind, CouplingFinding,
    CouplingFindingDetails, CouplingFindingKind, DryFinding, DryFindingDetails, DryFindingKind,
    DuplicateParticipant, IospFinding, SrpFinding, SrpFindingDetails, SrpFindingKind, TqFinding,
    TqFindingKind,
};
use crate::domain::Finding;
use crate::ports::reporter::ReporterImpl;
use crate::ports::Reporter;
use crate::report::ai::{
    format_arch_entry, format_complexity_entry, format_coupling_entry, format_dry_entry,
    format_iosp_entry, format_srp_entry, format_tq_entry, AiOutputFormat, AiReporter,
};
use crate::report::AnalysisResult;
use serde_json::Value;

/// Test-local helper: render via `AiReporter` with `Json` format and
/// parse back to `Value` so assertions can inspect structured data.
/// Lives here (not in production code) to keep the panic out of
/// production builds and avoid the architecture rule against loose
/// `#[cfg(test)]` items in production files.
fn build_ai_value(analysis: &AnalysisResult, config: &Config) -> Value {
    let reporter = AiReporter {
        config,
        data: &analysis.data,
        format: AiOutputFormat::Json,
    };
    let json_str = reporter.render(&analysis.findings, &analysis.data);
    serde_json::from_str(&json_str).expect("AiReporter::render(Json) must produce valid JSON")
}

fn empty_analysis() -> AnalysisResult {
    AnalysisResult {
        results: vec![],
        summary: crate::report::Summary::default(),
        findings: crate::domain::AnalysisFindings::default(),
        data: crate::domain::AnalysisData::default(),
    }
}

fn arch_common(file: &str, line: usize, severity: crate::domain::Severity) -> Finding {
    Finding {
        file: file.into(),
        line,
        column: 0,
        dimension: crate::findings::Dimension::Architecture,
        rule_id: "architecture/test".into(),
        message: "test".into(),
        severity,
        suppressed: false,
    }
}

// ── build_ai_value: shape contract ─────────────────────────────────

#[test]
fn build_ai_value_zero_findings_no_findings_by_file() {
    let analysis = empty_analysis();
    let config = Config::default();
    let value = build_ai_value(&analysis, &config);
    assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(value["findings"], 0);
    assert!(
        value.get("findings_by_file").is_none(),
        "no findings_by_file when 0 findings"
    );
}

#[test]
fn build_ai_value_includes_architecture_finding() {
    let mut analysis = empty_analysis();
    analysis.findings.architecture = vec![ArchitectureFinding {
        common: Finding {
            file: "src/cli/handlers.rs".into(),
            line: 17,
            column: 0,
            dimension: crate::findings::Dimension::Architecture,
            rule_id: "architecture/call_parity/no_delegation".into(),
            message: "cli pub fn delegates to no application function".into(),
            severity: crate::domain::Severity::Medium,
            suppressed: false,
        },
    }];
    let config = Config::default();
    let value = build_ai_value(&analysis, &config);

    assert_eq!(value["findings"], 1);
    let by_file = value["findings_by_file"]
        .as_object()
        .expect("findings_by_file present when findings > 0");
    let entries = by_file["src/cli/handlers.rs"]
        .as_array()
        .expect("entries for the architecture finding's file");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["category"], "architecture");
    assert_eq!(entries[0]["line"], 17);
}

#[test]
fn build_ai_value_groups_entries_by_file() {
    let mut analysis = empty_analysis();
    analysis.findings.architecture = vec![
        ArchitectureFinding {
            common: arch_common("src/a.rs", 10, crate::domain::Severity::Medium),
        },
        ArchitectureFinding {
            common: arch_common("src/a.rs", 20, crate::domain::Severity::Medium),
        },
        ArchitectureFinding {
            common: arch_common("src/b.rs", 5, crate::domain::Severity::Medium),
        },
    ];
    let config = Config::default();
    let value = build_ai_value(&analysis, &config);
    assert_eq!(value["findings"], 3);
    let by_file = value["findings_by_file"].as_object().expect("by_file");
    assert_eq!(by_file.len(), 2);
    assert!(by_file.contains_key("src/a.rs"));
    assert!(by_file.contains_key("src/b.rs"));
    assert_eq!(by_file["src/a.rs"].as_array().unwrap().len(), 2);
    assert_eq!(by_file["src/b.rs"].as_array().unwrap().len(), 1);
}

#[test]
fn build_ai_value_skips_suppressed() {
    let mut analysis = empty_analysis();
    let mut common = arch_common("src/foo.rs", 5, crate::domain::Severity::Medium);
    common.suppressed = true;
    analysis.findings.architecture = vec![ArchitectureFinding { common }];
    let config = Config::default();
    let value = build_ai_value(&analysis, &config);
    assert_eq!(value["findings"], 0);
    assert!(value.get("findings_by_file").is_none());
}

// ── AiReporter trait methods: per-dimension entry shape ────────────

fn make_reporter<'a>(config: &'a Config, data: &'a crate::domain::AnalysisData) -> AiReporter<'a> {
    AiReporter {
        config,
        data,
        format: AiOutputFormat::Json,
    }
}

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
    use crate::domain::analysis_data::FunctionClassification;
    let mut data = crate::domain::AnalysisData::default();
    data.functions.push(FunctionRecord {
        name: "bad_fn".into(),
        file: "src/lib.rs".into(),
        line: 40,
        qualified_name: "MyType::bad_fn".into(),
        parent_type: Some("MyType".into()),
        classification: FunctionClassification::Violation,
        severity: Some(crate::domain::Severity::Medium),
        complexity: None,
        parameter_count: 0,
        own_calls: vec![],
        is_trait_impl: false,
        is_test: false,
        effort_score: None,
        suppressed: false,
        complexity_suppressed: false,
    });
    let f = IospFinding {
        common: Finding {
            file: "src/lib.rs".into(),
            line: 40,
            column: 0,
            dimension: crate::findings::Dimension::Iosp,
            rule_id: "iosp/violation".into(),
            message: "x".into(),
            severity: crate::domain::Severity::Medium,
            suppressed: false,
        },
        logic_locations: vec![],
        call_locations: vec![],
        effort_score: None,
    };
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

// ── Smoke tests ────────────────────────────────────────────────────

#[test]
fn print_ai_no_findings_no_panic() {
    let analysis = empty_analysis();
    let config = Config::default();
    crate::report::ai::print_ai(&analysis, &config);
}

#[test]
fn print_ai_json_no_findings_no_panic() {
    let analysis = empty_analysis();
    let config = Config::default();
    crate::report::ai::print_ai_json(&analysis, &config);
}

#[test]
fn build_ai_value_with_complexity_finding_no_panic() {
    let mut analysis = empty_analysis();
    let _func = FunctionAnalysis {
        name: "f".into(),
        file: "test.rs".into(),
        line: 1,
        classification: Classification::Violation {
            has_logic: true,
            has_own_calls: true,
            logic_locations: vec![LogicOccurrence {
                kind: "if".into(),
                line: 2,
            }],
            call_locations: vec![CallOccurrence {
                name: "g".into(),
                line: 3,
            }],
        },
        parent_type: None,
        suppressed: false,
        complexity: Some(ComplexityMetrics {
            magic_numbers: vec![MagicNumberOccurrence {
                line: 4,
                value: "42".into(),
            }],
            ..Default::default()
        }),
        qualified_name: "f".into(),
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
    analysis.findings.complexity = vec![ComplexityFinding {
        common: Finding {
            file: "test.rs".into(),
            line: 4,
            column: 0,
            dimension: crate::findings::Dimension::Complexity,
            rule_id: "complexity/magic_number".into(),
            message: "magic number 42 in f".into(),
            severity: crate::domain::Severity::Medium,
            suppressed: false,
        },
        kind: ComplexityFindingKind::MagicNumber,
        metric_value: 1,
        threshold: 0,
        hotspot: None,
    }];
    let config = Config::default();
    let value = build_ai_value(&analysis, &config);
    assert_eq!(value["findings"], 1);
}

#[test]
fn ai_reporter_includes_orphan_entries_via_snapshot_view() {
    use crate::domain::findings::OrphanSuppression;
    let mut analysis = empty_analysis();
    analysis.findings.orphan_suppressions = vec![OrphanSuppression {
        file: "src/foo.rs".into(),
        line: 42,
        dimensions: vec![crate::findings::Dimension::Srp],
        reason: Some("legacy".into()),
    }];
    let config = Config::default();
    let value = build_ai_value(&analysis, &config);
    assert_eq!(
        value["findings"], 1,
        "orphan must count as a finding, got value: {value}"
    );
    let by_file = value["findings_by_file"]
        .as_object()
        .expect("findings_by_file present");
    let entries = by_file
        .get("src/foo.rs")
        .and_then(|v| v.as_array())
        .expect("entries for src/foo.rs");
    let orphan = entries
        .iter()
        .find(|e| e["category"] == "orphan_suppression")
        .expect("orphan entry under src/foo.rs");
    assert_eq!(orphan["line"], 42);
}
