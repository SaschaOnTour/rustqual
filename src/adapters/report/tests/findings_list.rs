//! findings_list reporter tests.
//!
//! Tests construct typed findings directly (via AnalysisFindings) rather
//! than the legacy dimension-specific fields, since the migrated
//! `collect_all_findings` reads from the typed source.

use crate::domain::findings::{
    ArchitectureFinding, ComplexityFinding, ComplexityFindingKind, CouplingFinding,
    CouplingFindingDetails, CouplingFindingKind, DryFinding, DryFindingDetails, DryFindingKind,
    DuplicateParticipant, IospFinding, TqFinding, TqFindingKind,
};
use crate::domain::Finding;
use crate::report::findings_list::*;
use crate::report::{AnalysisResult, Summary};

fn empty_analysis() -> AnalysisResult {
    AnalysisResult {
        results: vec![],
        summary: Summary::default(),
        findings: crate::domain::AnalysisFindings::default(),
        data: crate::domain::AnalysisData::default(),
    }
}

fn common(file: &str, line: usize, dim: crate::findings::Dimension) -> Finding {
    Finding {
        file: file.into(),
        line,
        column: 0,
        dimension: dim,
        rule_id: "test".into(),
        message: "test".into(),
        severity: crate::domain::Severity::Medium,
        suppressed: false,
    }
}

#[test]
fn test_collect_empty_analysis() {
    let analysis = empty_analysis();
    let findings = collect_all_findings(&analysis);
    assert!(findings.is_empty());
}

#[test]
fn test_collect_iosp_violation() {
    let mut analysis = empty_analysis();
    analysis.findings.iosp = vec![IospFinding {
        common: common("src/lib.rs", 5, crate::findings::Dimension::Iosp),
        logic_locations: vec![],
        call_locations: vec![],
        effort_score: None,
    }];
    let findings = collect_all_findings(&analysis);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].category, "VIOLATION");
}

#[test]
fn test_collect_magic_number_per_occurrence() {
    let mut analysis = empty_analysis();
    analysis.findings.complexity = vec![
        ComplexityFinding {
            common: common("src/lib.rs", 12, crate::findings::Dimension::Complexity),
            kind: ComplexityFindingKind::MagicNumber,
            metric_value: 1,
            threshold: 0,
            hotspot: None,
        },
        ComplexityFinding {
            common: common("src/lib.rs", 15, crate::findings::Dimension::Complexity),
            kind: ComplexityFindingKind::MagicNumber,
            metric_value: 1,
            threshold: 0,
            hotspot: None,
        },
    ];
    let findings = collect_all_findings(&analysis);
    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0].category, "MAGIC_NUMBER");
    assert_eq!(findings[0].line, 12);
    assert_eq!(findings[1].line, 15);
}

#[test]
fn test_sorted_by_file_and_line() {
    let mut analysis = empty_analysis();
    analysis.findings.complexity = vec![
        ComplexityFinding {
            common: common("src/b.rs", 20, crate::findings::Dimension::Complexity),
            kind: ComplexityFindingKind::ErrorHandling,
            metric_value: 1,
            threshold: 0,
            hotspot: None,
        },
        ComplexityFinding {
            common: common("src/a.rs", 10, crate::findings::Dimension::Complexity),
            kind: ComplexityFindingKind::ErrorHandling,
            metric_value: 1,
            threshold: 0,
            hotspot: None,
        },
    ];
    let findings = collect_all_findings(&analysis);
    assert_eq!(findings[0].file, "src/a.rs");
    assert_eq!(findings[1].file, "src/b.rs");
}

#[test]
fn test_suppressed_not_collected() {
    let mut analysis = empty_analysis();
    let mut c = common("src/lib.rs", 5, crate::findings::Dimension::Iosp);
    c.suppressed = true;
    analysis.findings.iosp = vec![IospFinding {
        common: c,
        logic_locations: vec![],
        call_locations: vec![],
        effort_score: None,
    }];
    let findings = collect_all_findings(&analysis);
    assert!(findings.is_empty());
}

#[test]
fn test_total_findings_dry_duplicate_per_participant() {
    let mut analysis = empty_analysis();
    let participants = vec![
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
    ];
    analysis.findings.dry = vec![
        DryFinding {
            common: common("src/a.rs", 10, crate::findings::Dimension::Dry),
            kind: DryFindingKind::DuplicateExact,
            details: DryFindingDetails::Duplicate {
                participants: participants.clone(),
            },
        },
        DryFinding {
            common: common("src/b.rs", 20, crate::findings::Dimension::Dry),
            kind: DryFindingKind::DuplicateExact,
            details: DryFindingDetails::Duplicate {
                participants: participants.clone(),
            },
        },
    ];
    let findings = collect_all_findings(&analysis);
    assert_eq!(findings.len(), 2);
    assert!(findings.iter().all(|f| f.category == "DUPLICATE"));
}

#[test]
fn test_total_findings_coupling_cycle_no_file() {
    let mut analysis = empty_analysis();
    let mut c = common("", 0, crate::findings::Dimension::Coupling);
    c.file = "".into();
    c.line = 0;
    analysis.findings.coupling = vec![CouplingFinding {
        common: c,
        kind: CouplingFindingKind::Cycle,
        details: CouplingFindingDetails::Cycle {
            modules: vec!["a".into(), "b".into(), "a".into()],
        },
    }];
    let findings = collect_all_findings(&analysis);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].category, "CYCLE");
    assert!(findings[0].file.is_empty());
}

#[test]
fn test_collect_architecture_finding() {
    let mut analysis = empty_analysis();
    analysis.findings.architecture = vec![ArchitectureFinding {
        common: common(
            "src/cli/handlers.rs",
            17,
            crate::findings::Dimension::Architecture,
        ),
    }];
    let findings = collect_all_findings(&analysis);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].category, "ARCHITECTURE");
    assert_eq!(findings[0].file, "src/cli/handlers.rs");
    assert_eq!(findings[0].line, 17);
}

#[test]
fn test_collect_test_quality_uncovered() {
    let mut analysis = empty_analysis();
    analysis.findings.test_quality = vec![TqFinding {
        common: common("src/lib.rs", 30, crate::findings::Dimension::TestQuality),
        kind: TqFindingKind::Uncovered,
        function_name: "uncovered_fn".into(),
        uncovered_lines: None,
    }];
    let findings = collect_all_findings(&analysis);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].category, "TQ_UNCOVERED");
}

#[test]
fn findings_list_includes_orphan_suppressions_via_snapshot_view() {
    use crate::domain::findings::OrphanSuppression;
    let mut analysis = empty_analysis();
    // Orphan suppressions now flow through `findings.orphan_suppressions`
    // (the trait-driven path) — the legacy `analysis.orphan_suppressions`
    // field is no longer the source for reporter rendering.
    analysis.findings.orphan_suppressions = vec![OrphanSuppression {
        file: "src/foo.rs".into(),
        line: 42,
        dimensions: vec![crate::findings::Dimension::Srp],
        reason: Some("legacy marker".into()),
    }];
    let findings = collect_all_findings(&analysis);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].category, "ORPHAN_SUPPRESSION");
    assert!(findings[0].detail.contains("srp"));
    assert!(findings[0].detail.contains("legacy marker"));
}

#[test]
fn test_print_findings_empty_no_panic() {
    print_findings(&[]);
}

#[test]
fn test_print_findings_with_entries_no_panic() {
    let entries = vec![FindingEntry::new(
        "src/foo.rs",
        10,
        "VIOLATION",
        "logic + calls".into(),
        "fn_x".into(),
    )];
    print_findings(&entries);
}
