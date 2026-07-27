use crate::adapters::analyzers::iosp::{
    CallOccurrence, Classification, ComplexityMetrics, FunctionAnalysis, LogicOccurrence,
};
use crate::adapters::report::test_support::make_result;
use crate::report::text::*;
use crate::report::Summary;

fn render_text(results: &[FunctionAnalysis], verbose: bool) -> String {
    use crate::domain::{AnalysisData, AnalysisFindings};
    use crate::ports::Reporter;
    let summary = Summary::from_results(results);
    let reporter = TextReporter {
        summary: &summary,
        function_analyses: results,
        findings_entries: &[],
        verbose,
        suggestions_text: None,
        has_coverage: true,
    };
    let findings = AnalysisFindings::default();
    let data = AnalysisData::default();
    reporter.render(&findings, &data)
}

#[test]
fn test_print_report_empty_emits_quality_score_header() {
    let out = render_text(&[], false);
    assert!(
        out.to_lowercase().contains("quality") || out.contains("Score"),
        "empty report must still emit a quality summary section; got {out}"
    );
}

#[test]
fn test_print_report_no_violations_marks_clean() {
    let out = render_text(
        &[make_result("good_fn", Classification::Integration)],
        false,
    );
    // Clean analysis: summary section produced, no Violation row
    assert!(
        !out.to_uppercase().contains("VIOLATION"),
        "Integration-only analysis must not surface VIOLATION rows; got {out}"
    );
}

#[test]
fn test_print_report_with_violation_surfaces_bad_fn() {
    let out = render_text(
        &[make_result(
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
        )],
        true,
    );
    assert!(
        out.contains("bad_fn"),
        "verbose text report must mention the violating function name; got {out}"
    );
}

#[test]
fn test_print_report_verbose_lists_all_classifications() {
    let out = render_text(
        &[
            make_result("integrate_fn", Classification::Integration),
            make_result("operate_fn", Classification::Operation),
            make_result("trivial_fn", Classification::Trivial),
            make_result(
                "violate_fn",
                Classification::Violation {
                    has_logic: true,
                    has_own_calls: true,
                    logic_locations: vec![LogicOccurrence {
                        kind: "for".into(),
                        line: 1,
                    }],
                    call_locations: vec![CallOccurrence {
                        name: "foo".into(),
                        line: 2,
                    }],
                },
            ),
        ],
        true,
    );
    // Verbose mode lists every function — assert each name appears.
    for name in &["integrate_fn", "operate_fn", "trivial_fn", "violate_fn"] {
        assert!(
            out.contains(name),
            "verbose output missing `{name}`; got {out}"
        );
    }
}

#[test]
fn test_print_report_with_complexity_surfaces_complexity_metrics() {
    let mut func = make_result("complex_fn", Classification::Operation);
    func.complexity = Some(ComplexityMetrics {
        logic_count: 5,
        call_count: 0,
        max_nesting: 3,
        ..Default::default()
    });
    let out = render_text(&[func], true);
    assert!(
        out.contains("complex_fn"),
        "verbose output must list the function; got {out}"
    );
    assert!(
        out.contains("nesting=3") || out.contains("logic=5"),
        "verbose output must surface complexity metrics; got {out}"
    );
}

#[test]
fn text_reporter_renders_orphans_via_snapshot_view() {
    // Verify the migration: orphan rendering must come from
    // `snapshot.orphans` (the trait-driven view), not from the legacy
    // `findings_entries` struct-field bypass. We construct a TextReporter
    // with an EMPTY findings_entries field and populate ONLY
    // `findings.orphan_suppressions`. If the verbose path still emits
    // the orphan section, it must have come through `build_orphans` →
    // `Snapshot::orphans` → `publish`. RED before the migration (no-op
    // build_orphans + verbose path reads findings_entries).
    use crate::domain::findings::OrphanSuppression;
    use crate::domain::{AnalysisData, AnalysisFindings, Dimension};
    use crate::ports::Reporter;
    let summary = Summary::from_results(&[]);
    let reporter = TextReporter {
        summary: &summary,
        function_analyses: &[],
        findings_entries: &[],
        verbose: true,
        suggestions_text: None,
        has_coverage: true,
    };
    let findings = AnalysisFindings {
        orphan_suppressions: vec![OrphanSuppression {
            marker: crate::domain::findings::MarkerKind::Allow,
            file: "src/foo.rs".to_string(),
            line: 42,
            dimensions: vec![Dimension::Iosp],
            reason: Some("legacy".to_string()),
            target: None,
            kind: crate::domain::findings::OrphanKind::Stale,
        }],
        ..Default::default()
    };
    let data = AnalysisData::default();
    let output = reporter.render(&findings, &data);
    assert!(
        output.contains("Orphan Suppression"),
        "verbose text output must render orphan section from snapshot.orphans (not from findings_entries struct field), got:\n{output}"
    );
    assert!(
        output.contains("src/foo.rs:42"),
        "orphan entry must appear with file:line, got:\n{output}"
    );
}

#[test]
fn test_print_report_suppressed_verbose_marks_function() {
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
    let out = render_text(&[func], true);
    assert!(
        out.contains("suppressed_fn"),
        "verbose mode must list the function even when suppressed; got {out}"
    );
}

#[test]
fn footer_with_findings_points_at_rule_cards_and_allow_guide() {
    // The epilogue is the only part guaranteed to survive an agent's
    // `| tail -30` — it must carry the next step for BOTH paths: rule
    // lookup (--explain <RULE-ID>) and suppression syntax (--explain allow).
    use crate::domain::{AnalysisData, AnalysisFindings};
    use crate::ports::Reporter;
    use crate::report::findings_list::FindingEntry;
    let entries = [FindingEntry::new(
        "src/x.rs",
        1,
        "BOILERPLATE",
        "BP-009".into(),
        "build_thing".into(),
    )];
    let summary = Summary::from_results(&[]);
    let reporter = TextReporter {
        summary: &summary,
        function_analyses: &[],
        findings_entries: &entries,
        verbose: false,
        suggestions_text: None,
        has_coverage: true,
    };
    let out = reporter.render(&AnalysisFindings::default(), &AnalysisData::default());
    assert!(
        out.contains("rustqual --explain <RULE-ID>"),
        "footer must point at rule cards: {out}"
    );
    assert!(
        out.contains("rustqual --explain allow"),
        "footer must keep pointing at the allow guide: {out}"
    );
    let findings_heading = out.find("Finding").expect("findings list rendered");
    let hint = out
        .rfind("--explain <RULE-ID>")
        .expect("rule-card hint rendered");
    assert!(
        hint > findings_heading,
        "the explain hint must render after the findings list so tail keeps it"
    );
}

#[test]
fn orphan_not_double_counted_when_present_in_findings_entries() {
    // Production feeds TextReporter `findings_entries = collect_all_findings(...)`,
    // which ALREADY includes orphan entries. The compact path must not append
    // the snapshot orphans on top, or every orphan renders twice (issue #36:
    // 3 markers → 6 ORPHAN_SUPPRESSION lines). Reproduce faithfully: the same
    // orphan is in BOTH findings_entries and findings.orphan_suppressions.
    use crate::domain::findings::{OrphanKind, OrphanSuppression};
    use crate::domain::{AnalysisData, AnalysisFindings, Dimension};
    use crate::ports::Reporter;
    use crate::report::findings_list::orphan_to_finding_entry;

    let orphan = OrphanSuppression {
        marker: crate::domain::findings::MarkerKind::Allow,
        file: "src/lib.rs".to_string(),
        line: 9,
        dimensions: vec![Dimension::Dry],
        reason: Some("intended duplication".to_string()),
        target: None,
        kind: OrphanKind::Stale,
    };
    let findings_entries = [orphan_to_finding_entry(&orphan)];
    let summary = Summary::from_results(&[]);
    let reporter = TextReporter {
        summary: &summary,
        function_analyses: &[],
        findings_entries: &findings_entries,
        verbose: false,
        suggestions_text: None,
        has_coverage: true,
    };
    let findings = AnalysisFindings {
        orphan_suppressions: vec![orphan],
        ..Default::default()
    };
    let output = reporter.render(&findings, &AnalysisData::default());
    let hits = output.matches("ORPHAN_SUPPRESSION").count();
    assert_eq!(
        hits, 1,
        "orphan must render exactly once, not once per source; got {hits}:\n{output}"
    );
}

#[test]
fn untested_findings_without_coverage_say_so() {
    // Without `--coverage`, TQ-003 is answered from the call graph, which
    // cannot follow a macro it does not expand. The reader has to know which of
    // the two answers they are looking at before deleting anything.
    let entries = vec![FindingEntry {
        file: "src/lib.rs".into(),
        line: 3,
        category: "TQ_UNTESTED",
        function_name: "helper".into(),
        detail: String::new(),
    }];
    let with_hint = coverage_hint(&entries, false);
    assert!(
        with_hint.contains("--coverage"),
        "must name the flag: {with_hint}"
    );
    assert!(
        coverage_hint(&entries, true).is_empty(),
        "a measured answer needs no hint"
    );
    let others = vec![FindingEntry {
        category: "DEAD_CODE",
        ..entries[0].clone()
    }];
    assert!(
        coverage_hint(&others, false).is_empty(),
        "only the check that depends on coverage says it"
    );
}
