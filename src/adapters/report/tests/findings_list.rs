use crate::adapters::analyzers::iosp::{
    Classification, ComplexityMetrics, FunctionAnalysis, MagicNumberOccurrence,
};
use crate::report::findings_list::*;
use crate::report::{AnalysisResult, Summary};

fn make_fa(name: &str, file: &str, line: usize) -> FunctionAnalysis {
    FunctionAnalysis {
        name: name.to_string(),
        file: file.to_string(),
        line,
        classification: Classification::Operation,
        parent_type: None,
        suppressed: false,
        complexity: None,
        qualified_name: name.to_string(),
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
    }
}

fn empty_analysis() -> AnalysisResult {
    AnalysisResult {
        results: vec![],
        summary: Summary::default(),
        coupling: None,
        duplicates: vec![],
        dead_code: vec![],
        fragments: vec![],
        boilerplate: vec![],
        wildcard_warnings: vec![],
        repeated_matches: vec![],
        srp: None,
        tq: None,
        structural: None,
        architecture_findings: vec![],
        orphan_suppressions: vec![],
    }
}

#[test]
fn test_collect_empty_analysis() {
    let analysis = empty_analysis();
    let findings = collect_all_findings(&analysis);
    assert!(findings.is_empty());
}

#[test]
fn test_collect_magic_numbers() {
    let mut analysis = empty_analysis();
    let mut fa = make_fa("test_fn", "src/lib.rs", 10);
    fa.complexity = Some(ComplexityMetrics {
        magic_numbers: vec![
            MagicNumberOccurrence {
                line: 12,
                value: "42".to_string(),
            },
            MagicNumberOccurrence {
                line: 15,
                value: "99".to_string(),
            },
        ],
        ..Default::default()
    });
    analysis.results = vec![fa];
    let findings = collect_all_findings(&analysis);
    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0].category, "MAGIC_NUMBER");
    assert_eq!(findings[0].detail, "42");
    assert_eq!(findings[1].detail, "99");
}

#[test]
fn test_collect_violation() {
    let mut analysis = empty_analysis();
    let mut fa = make_fa("bad_fn", "src/lib.rs", 5);
    fa.classification = Classification::Violation {
        has_logic: true,
        has_own_calls: true,
        logic_locations: vec![],
        call_locations: vec![],
    };
    analysis.results = vec![fa];
    let findings = collect_all_findings(&analysis);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].category, "VIOLATION");
}

#[test]
fn test_sorted_by_file_and_line() {
    let mut analysis = empty_analysis();
    let mut fa1 = make_fa("fn_b", "src/b.rs", 20);
    fa1.error_handling_warning = true;
    fa1.complexity = Some(ComplexityMetrics::default());
    let mut fa2 = make_fa("fn_a", "src/a.rs", 10);
    fa2.error_handling_warning = true;
    fa2.complexity = Some(ComplexityMetrics::default());
    analysis.results = vec![fa1, fa2];
    let findings = collect_all_findings(&analysis);
    assert_eq!(findings[0].file, "src/a.rs");
    assert_eq!(findings[1].file, "src/b.rs");
}

#[test]
fn test_suppressed_not_collected() {
    let mut analysis = empty_analysis();
    let mut fa = make_fa("suppressed_fn", "src/lib.rs", 5);
    fa.suppressed = true;
    fa.classification = Classification::Violation {
        has_logic: true,
        has_own_calls: true,
        logic_locations: vec![],
        call_locations: vec![],
    };
    analysis.results = vec![fa];
    let findings = collect_all_findings(&analysis);
    assert!(findings.is_empty());
}

// ── Contract tests: when summary counts match per-entry semantics,
// total_findings() must equal collect_all_findings().len().
// Pipeline integration is tested by test_self_analysis_no_violations. ──

#[test]
fn test_total_findings_consistent_magic_numbers() {
    let mut analysis = empty_analysis();
    let mut fa = make_fa("fn1", "src/lib.rs", 10);
    fa.complexity = Some(ComplexityMetrics {
        magic_numbers: vec![
            MagicNumberOccurrence {
                line: 12,
                value: "42".to_string(),
            },
            MagicNumberOccurrence {
                line: 15,
                value: "99".to_string(),
            },
        ],
        ..Default::default()
    });
    analysis.results = vec![fa];
    // Pipeline must count per-occurrence, not per-function
    analysis.summary.magic_number_warnings = 2;
    let findings = collect_all_findings(&analysis);
    assert_eq!(
        analysis.summary.total_findings(),
        findings.len(),
        "total_findings() must equal collect_all_findings().len()"
    );
}

#[test]
fn test_total_findings_consistent_duplicates() {
    use crate::adapters::analyzers::dry::functions::{
        DuplicateEntry, DuplicateGroup, DuplicateKind,
    };
    let mut analysis = empty_analysis();
    analysis.duplicates = vec![DuplicateGroup {
        entries: vec![
            DuplicateEntry {
                name: "fn_a".to_string(),
                qualified_name: "mod::fn_a".to_string(),
                file: "src/a.rs".to_string(),
                line: 10,
            },
            DuplicateEntry {
                name: "fn_b".to_string(),
                qualified_name: "mod::fn_b".to_string(),
                file: "src/b.rs".to_string(),
                line: 20,
            },
        ],
        kind: DuplicateKind::Exact,
        suppressed: false,
    }];
    // Pipeline must count per-entry (2), not per-group (1)
    analysis.summary.duplicate_groups = 2;
    let findings = collect_all_findings(&analysis);
    assert_eq!(
        analysis.summary.total_findings(),
        findings.len(),
        "total_findings() must equal collect_all_findings().len()"
    );
}

#[test]
fn test_total_findings_consistent_fragments() {
    use crate::adapters::analyzers::dry::fragments::{FragmentEntry, FragmentGroup};
    let mut analysis = empty_analysis();
    analysis.fragments = vec![FragmentGroup {
        entries: vec![
            FragmentEntry {
                function_name: "fn_a".to_string(),
                qualified_name: "mod::fn_a".to_string(),
                file: "src/a.rs".to_string(),
                start_line: 10,
                end_line: 15,
            },
            FragmentEntry {
                function_name: "fn_b".to_string(),
                qualified_name: "mod::fn_b".to_string(),
                file: "src/b.rs".to_string(),
                start_line: 20,
                end_line: 25,
            },
            FragmentEntry {
                function_name: "fn_c".to_string(),
                qualified_name: "mod::fn_c".to_string(),
                file: "src/c.rs".to_string(),
                start_line: 30,
                end_line: 35,
            },
        ],
        statement_count: 3,
        suppressed: false,
    }];
    // Pipeline must count per-entry (3), not per-group (1)
    analysis.summary.fragment_groups = 3;
    let findings = collect_all_findings(&analysis);
    assert_eq!(
        analysis.summary.total_findings(),
        findings.len(),
        "total_findings() must equal collect_all_findings().len()"
    );
}

#[test]
fn test_total_findings_consistent_mixed() {
    use crate::adapters::analyzers::dry::functions::{
        DuplicateEntry, DuplicateGroup, DuplicateKind,
    };
    let mut analysis = empty_analysis();
    // 1 function with 2 magic numbers
    let mut fa = make_fa("fn1", "src/lib.rs", 10);
    fa.complexity = Some(ComplexityMetrics {
        magic_numbers: vec![
            MagicNumberOccurrence {
                line: 12,
                value: "400".to_string(),
            },
            MagicNumberOccurrence {
                line: 13,
                value: "800".to_string(),
            },
        ],
        ..Default::default()
    });
    analysis.results = vec![fa];
    // 1 duplicate group with 2 entries
    analysis.duplicates = vec![DuplicateGroup {
        entries: vec![
            DuplicateEntry {
                name: "fn_a".to_string(),
                qualified_name: "mod::fn_a".to_string(),
                file: "src/a.rs".to_string(),
                line: 100,
            },
            DuplicateEntry {
                name: "fn_b".to_string(),
                qualified_name: "mod::fn_b".to_string(),
                file: "src/b.rs".to_string(),
                line: 200,
            },
        ],
        kind: DuplicateKind::Exact,
        suppressed: false,
    }];
    analysis.summary.magic_number_warnings = 2;
    analysis.summary.duplicate_groups = 2;
    let findings = collect_all_findings(&analysis);
    // 2 magic numbers + 2 duplicate entries = 4 findings
    assert_eq!(findings.len(), 4);
    assert_eq!(
        analysis.summary.total_findings(),
        findings.len(),
        "total_findings() must equal collect_all_findings().len() — was the bug from issue report"
    );
}

#[test]
fn test_total_findings_consistent_coupling() {
    let mut analysis = empty_analysis();
    analysis.coupling = Some(crate::adapters::analyzers::coupling::CouplingAnalysis {
        metrics: vec![crate::adapters::analyzers::coupling::CouplingMetrics {
            module_name: "db".to_string(),
            afferent: 2,
            efferent: 5,
            instability: 0.71,
            incoming: vec![],
            outgoing: vec![],
            suppressed: false,
            warning: true,
        }],
        cycles: vec![crate::adapters::analyzers::coupling::CycleReport {
            modules: vec!["a".to_string(), "b".to_string()],
        }],
        sdp_violations: vec![],
        graph: crate::adapters::analyzers::coupling::ModuleGraph::default(),
    });
    // 1 coupling warning + 1 cycle = 2
    analysis.summary.coupling_warnings = 1;
    analysis.summary.coupling_cycles = 1;
    let findings = collect_all_findings(&analysis);
    assert_eq!(
        analysis.summary.total_findings(),
        findings.len(),
        "coupling warnings and cycles must appear in findings list"
    );
    assert!(
        findings.iter().any(|f| f.category == "COUPLING"
            && f.function_name == "db"
            && f.detail.contains("I=0.71")),
        "expected a COUPLING finding for db with instability detail"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.category == "CYCLE" && f.detail.contains("a > b")),
        "expected a CYCLE finding describing the a > b cycle"
    );
}

// ── Orphan-suppression findings ──────────────────────────────

#[test]
fn orphan_suppressions_are_emitted_as_findings() {
    use crate::adapters::report::OrphanSuppressionWarning;
    let mut analysis = empty_analysis();
    analysis.orphan_suppressions = vec![OrphanSuppressionWarning {
        file: "src/foo.rs".into(),
        line: 42,
        dimensions: vec![crate::findings::Dimension::Srp],
        reason: Some("stale marker".into()),
    }];
    let findings = collect_all_findings(&analysis);
    let orphan: Vec<&_> = findings
        .iter()
        .filter(|f| f.category == "ORPHAN_SUPPRESSION")
        .collect();
    assert_eq!(orphan.len(), 1, "one orphan finding expected");
    assert_eq!(orphan[0].file, "src/foo.rs");
    assert_eq!(orphan[0].line, 42);
    assert!(
        orphan[0].detail.contains("srp"),
        "detail should name the suppressed dimension(s), got: {:?}",
        orphan[0].detail
    );
}

#[test]
fn orphan_finding_detail_lists_all_dimensions() {
    use crate::adapters::report::OrphanSuppressionWarning;
    let mut analysis = empty_analysis();
    analysis.orphan_suppressions = vec![OrphanSuppressionWarning {
        file: "src/foo.rs".into(),
        line: 5,
        dimensions: vec![
            crate::findings::Dimension::Iosp,
            crate::findings::Dimension::Complexity,
        ],
        reason: None,
    }];
    let findings = collect_all_findings(&analysis);
    let orphan = findings
        .iter()
        .find(|f| f.category == "ORPHAN_SUPPRESSION")
        .expect("orphan finding");
    assert!(
        orphan.detail.contains("iosp") && orphan.detail.contains("complexity"),
        "detail should name both dims, got: {:?}",
        orphan.detail
    );
}

#[test]
fn bare_orphan_detail_says_wildcard() {
    use crate::adapters::report::OrphanSuppressionWarning;
    let mut analysis = empty_analysis();
    analysis.orphan_suppressions = vec![OrphanSuppressionWarning {
        file: "src/foo.rs".into(),
        line: 5,
        dimensions: vec![],
        reason: None,
    }];
    let findings = collect_all_findings(&analysis);
    let orphan = findings
        .iter()
        .find(|f| f.category == "ORPHAN_SUPPRESSION")
        .expect("orphan finding");
    assert!(
        orphan.detail.to_lowercase().contains("all dims")
            || orphan.detail.to_lowercase().contains("wildcard"),
        "bare orphan detail should indicate wildcard semantics, got: {:?}",
        orphan.detail
    );
}
