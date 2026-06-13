use crate::domain::findings::{
    ArchitectureFinding, ComplexityFinding, ComplexityFindingKind, CouplingFinding,
    CouplingFindingDetails, CouplingFindingKind, DryFinding, DryFindingDetails, DryFindingKind,
    IospFinding, SrpFinding, SrpFindingDetails, SrpFindingKind, TqFinding, TqFindingKind,
};
use crate::domain::{AnalysisData, AnalysisFindings, Finding, Severity};
use crate::report::sarif::build_sarif_value;
use crate::report::sarif::rules::*;
use crate::report::{AnalysisResult, Summary};
use std::collections::HashSet;

#[test]
fn test_sarif_rules_contain_boilerplate_patterns() {
    let rules = sarif_rules();
    let ids: Vec<&str> = rules.iter().filter_map(|r| r["id"].as_str()).collect();
    for bp in [
        "BP-001", "BP-002", "BP-003", "BP-004", "BP-005", "BP-006", "BP-007", "BP-008", "BP-009",
        "BP-010",
    ] {
        assert!(ids.contains(&bp), "SARIF rules should contain {bp}");
    }
}

fn finding_with_rule_id(rule_id: &str) -> Finding {
    Finding {
        file: "src/test.rs".into(),
        line: 1,
        column: 0,
        dimension: crate::findings::Dimension::Architecture,
        rule_id: rule_id.into(),
        message: "x".into(),
        severity: Severity::Medium,
        suppressed: false,
    }
}

fn make_analysis_for(findings: AnalysisFindings) -> AnalysisResult {
    AnalysisResult {
        results: vec![],
        summary: Summary::default(),
        findings,
        data: AnalysisData::default(),
    }
}

/// A `Finding` for `rule_id` with its dimension set to `kind`.
fn dim_finding(kind: crate::findings::Dimension, rule_id: &str) -> Finding {
    let mut f = finding_with_rule_id(rule_id);
    f.dimension = kind;
    f
}

/// The three DRY variants whose rule_ids were historically wrong: boilerplate
/// (BP-007), wildcard (DRY-004), repeated-match (DRY-005).
fn registry_dry_findings() -> Vec<DryFinding> {
    vec![
        DryFinding {
            common: dim_finding(crate::findings::Dimension::Dry, "dry/boilerplate"),
            kind: DryFindingKind::Boilerplate,
            details: DryFindingDetails::Boilerplate {
                pattern_id: "BP-007".into(),
                struct_name: None,
                suggestion: "use thiserror".into(),
            },
        },
        DryFinding {
            common: dim_finding(crate::findings::Dimension::Dry, "dry/wildcard"),
            kind: DryFindingKind::Wildcard,
            details: DryFindingDetails::Wildcard {
                module_path: "foo".into(),
            },
        },
        DryFinding {
            common: dim_finding(crate::findings::Dimension::Dry, "dry/repeated_match"),
            kind: DryFindingKind::RepeatedMatch,
            details: DryFindingDetails::RepeatedMatch {
                enum_name: "Color".into(),
                participants: vec![],
            },
        },
    ]
}

/// One finding per dimension/variant whose rule_id was historically wrong
/// (BP-007, DRY-004, DRY-005, BTC, CP-002, the dynamic architecture id, IOSP).
fn registry_coverage_analysis() -> AnalysisResult {
    let srp = vec![SrpFinding {
        common: dim_finding(crate::findings::Dimension::Srp, "srp/structural"),
        kind: SrpFindingKind::Structural,
        details: SrpFindingDetails::Structural {
            item_name: "Foo".into(),
            code: "BTC".into(),
            detail: "x".into(),
        },
    }];
    let coupling = vec![CouplingFinding {
        common: dim_finding(crate::findings::Dimension::Coupling, "coupling/threshold"),
        kind: CouplingFindingKind::ThresholdExceeded,
        details: CouplingFindingDetails::ThresholdExceeded {
            module_name: "m".into(),
            instability: 0.9,
            afferent: 1,
            efferent: 9,
        },
    }];
    let architecture = vec![ArchitectureFinding {
        common: dim_finding(
            crate::findings::Dimension::Architecture,
            "architecture/pattern/forbid_path_prefix",
        ),
    }];
    let iosp = vec![IospFinding {
        common: dim_finding(crate::findings::Dimension::Iosp, "iosp/violation"),
        logic_locations: vec![],
        call_locations: vec![],
        effort_score: None,
    }];
    make_analysis_for(AnalysisFindings {
        iosp,
        dry: registry_dry_findings(),
        srp,
        coupling,
        architecture,
        ..Default::default()
    })
}

#[test]
fn every_emitted_rule_id_is_registered_in_rules_table() {
    let analysis = registry_coverage_analysis();
    let value = build_sarif_value(&analysis);
    let registered: HashSet<String> = value["runs"][0]["tool"]["driver"]["rules"]
        .as_array()
        .expect("rules")
        .iter()
        .filter_map(|r| r["id"].as_str().map(|s| s.to_string()))
        .collect();
    let emitted: HashSet<String> = value["runs"][0]["results"]
        .as_array()
        .expect("results")
        .iter()
        .filter_map(|r| r["ruleId"].as_str().map(|s| s.to_string()))
        .collect();
    let missing: Vec<&String> = emitted
        .iter()
        .filter(|id| !registered.contains(*id))
        .collect();
    assert!(
        missing.is_empty(),
        "every emitted ruleId must be in the rules table; missing: {missing:?}"
    );
    // Specifically check the dynamic architecture id was added.
    assert!(
        registered.contains("architecture/pattern/forbid_path_prefix"),
        "dynamic architecture id should be added to rules table"
    );
    // IOSP must use the canonical `iosp/violation` rule_id (not the
    // historical `A01` shorthand) so the static `helpUri` from the
    // catalogue is reachable.
    assert!(
        emitted.contains("iosp/violation"),
        "IOSP findings must emit canonical rule_id `iosp/violation`"
    );
}

fn cxf(kind: ComplexityFindingKind, suppressed: bool) -> ComplexityFinding {
    let mut common = dim_finding(crate::findings::Dimension::Complexity, "cx");
    common.suppressed = suppressed;
    ComplexityFinding {
        common,
        kind,
        metric_value: 0,
        threshold: 0,
        hotspot: None,
    }
}

fn tqf(kind: TqFindingKind, suppressed: bool) -> TqFinding {
    let mut common = dim_finding(crate::findings::Dimension::TestQuality, "tq");
    common.suppressed = suppressed;
    TqFinding {
        common,
        kind,
        function_name: "t".into(),
        uncovered_lines: None,
    }
}

fn drf(kind: DryFindingKind, suppressed: bool) -> DryFinding {
    let mut common = dim_finding(crate::findings::Dimension::Dry, "dry");
    common.suppressed = suppressed;
    DryFinding {
        common,
        kind,
        details: DryFindingDetails::DeadCode {
            qualified_name: String::new(),
            suggestion: None,
        },
    }
}

fn cpf(details: CouplingFindingDetails, suppressed: bool) -> CouplingFinding {
    let mut common = dim_finding(crate::findings::Dimension::Coupling, "cp");
    common.suppressed = suppressed;
    CouplingFinding {
        common,
        kind: CouplingFindingKind::Structural,
        details,
    }
}

#[test]
fn sarif_results_exclude_suppressed_findings_and_emit_ratio() {
    // Each dimension carries one unsuppressed + one suppressed finding with
    // distinct rule ids. Only the unsuppressed ids reach the results array
    // (pins the five `!suppressed` filters), and an exceeded suppression ratio
    // emits the SUP-001 result (pins `suppression_ratio_result`).
    let findings = AnalysisFindings {
        complexity: vec![
            cxf(ComplexityFindingKind::Cognitive, false),
            cxf(ComplexityFindingKind::Unsafe, true),
        ],
        dry: vec![
            drf(DryFindingKind::DuplicateExact, false),
            drf(DryFindingKind::DeadCodeUncalled, true),
        ],
        srp: vec![
            srp_kind(SrpFindingKind::StructCohesion, false),
            srp_kind(SrpFindingKind::ParameterCount, true),
        ],
        coupling: vec![
            cpf(CouplingFindingDetails::Cycle { modules: vec![] }, false),
            cpf(
                CouplingFindingDetails::ThresholdExceeded {
                    module_name: String::new(),
                    afferent: 0,
                    efferent: 0,
                    instability: 0.0,
                },
                true,
            ),
        ],
        test_quality: vec![
            tqf(TqFindingKind::NoAssertion, false),
            tqf(TqFindingKind::Uncovered, true),
        ],
        ..Default::default()
    };
    let mut analysis = make_analysis_for(findings);
    analysis.summary.suppression_ratio_exceeded = true;
    let value = build_sarif_value(&analysis);
    let emitted: HashSet<String> = value["runs"][0]["results"]
        .as_array()
        .expect("results")
        .iter()
        .filter_map(|r| r["ruleId"].as_str().map(str::to_string))
        .collect();
    for present in [
        "CX-001", "DRY-001", "SRP-001", "CP-001", "TQ-001", "SUP-001",
    ] {
        assert!(
            emitted.contains(present),
            "{present} must be emitted: {emitted:?}"
        );
    }
    for absent in ["CX-006", "DRY-002", "SRP-003", "CP-003"] {
        assert!(
            !emitted.contains(absent),
            "suppressed {absent} must be excluded: {emitted:?}"
        );
    }
}

fn srp_kind(kind: SrpFindingKind, suppressed: bool) -> SrpFinding {
    let mut common = dim_finding(crate::findings::Dimension::Srp, "srp");
    common.suppressed = suppressed;
    SrpFinding {
        common,
        kind,
        details: SrpFindingDetails::ParameterCount {
            function_name: String::new(),
            parameter_count: 0,
        },
    }
}

#[test]
fn sarif_rules_render_from_the_rule_card_registry() {
    // Single source of truth: the SARIF rules table and the rule-card
    // registry must agree exactly — a rule added to one but not the other
    // would drift the catalog.
    let rules = sarif_rules();
    let sarif_ids: HashSet<&str> = rules.iter().filter_map(|r| r["id"].as_str()).collect();
    let card_ids: HashSet<&str> = crate::domain::rule_cards::all_rule_cards()
        .map(|c| c.id)
        .collect();
    let missing_cards: Vec<&&str> = sarif_ids.difference(&card_ids).collect();
    let missing_sarif: Vec<&&str> = card_ids.difference(&sarif_ids).collect();
    assert!(
        missing_cards.is_empty() && missing_sarif.is_empty(),
        "registry drift — sarif-only: {missing_cards:?}, cards-only: {missing_sarif:?}"
    );
}
