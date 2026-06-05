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
    };
    let findings = AnalysisFindings {
        orphan_suppressions: vec![OrphanSuppression {
            file: "src/foo.rs".to_string(),
            line: 42,
            dimensions: vec![Dimension::Iosp],
            reason: Some("legacy".to_string()),
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
