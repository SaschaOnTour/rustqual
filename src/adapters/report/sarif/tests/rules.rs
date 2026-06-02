use crate::domain::findings::{
    ArchitectureFinding, CouplingFinding, CouplingFindingDetails, CouplingFindingKind, DryFinding,
    DryFindingDetails, DryFindingKind, IospFinding, SrpFinding, SrpFindingDetails, SrpFindingKind,
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
