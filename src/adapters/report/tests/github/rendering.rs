use super::*;

// ── new ReporterImpl interface ──────────────────────────────────

#[test]
fn test_github_render_includes_summary_annotation() {
    use crate::ports::Reporter;
    let summary = Summary {
        total: 100,
        quality_score: 1.0,
        ..Default::default()
    };
    let reporter = GithubReporter { summary: &summary };
    let findings = crate::domain::AnalysisFindings::default();
    let data = crate::domain::AnalysisData::default();
    let out = reporter.render(&findings, &data);
    assert!(
        out.contains("::notice"),
        "render output must include summary annotation, got: {out}",
    );
    assert!(out.contains("100.0%"));
}

#[test]
fn test_github_render_emits_iosp_annotation_then_summary() {
    use crate::domain::findings::{CallLocation, IospFinding, LogicLocation};
    use crate::ports::Reporter;
    let summary = Summary {
        total: 1,
        violations: 1,
        quality_score: 0.5,
        ..Default::default()
    };
    let mut findings = crate::domain::AnalysisFindings::default();
    findings.iosp.push(IospFinding {
        common: Finding {
            file: "src/lib.rs".into(),
            line: 17,
            column: 0,
            dimension: crate::findings::Dimension::Iosp,
            rule_id: "iosp/violation".into(),
            message: "x".into(),
            severity: crate::domain::Severity::Medium,
            suppressed: false,
        },
        logic_locations: vec![LogicLocation {
            kind: "if".into(),
            line: 18,
        }],
        call_locations: vec![CallLocation {
            name: "h".into(),
            line: 19,
        }],
        effort_score: None,
    });
    let reporter = GithubReporter { summary: &summary };
    let out = reporter.render(&findings, &crate::domain::AnalysisData::default());
    let iosp_pos = out
        .find("file=src/lib.rs")
        .expect("iosp annotation missing");
    let summary_pos = out
        .find("::error::Quality analysis")
        .expect("summary missing");
    assert!(
        iosp_pos < summary_pos,
        "per-dim chunks must come before summary annotation in render output",
    );
}

#[test]
fn architecture_message_with_special_chars_is_escaped() {
    // GitHub workflow commands break on `%`, CR, LF in message bodies
    // and on `,`/`:` in property values. The annotation must escape
    // them so config-provided reason text or path fragments cannot
    // corrupt the output or split it into a second workflow command.
    let mut common = Finding {
        file: "src/foo,with,comma.rs".into(),
        line: 7,
        column: 0,
        dimension: crate::findings::Dimension::Architecture,
        rule_id: "architecture/custom".into(),
        message: "100% bad\nline2".into(),
        severity: crate::domain::Severity::Medium,
        suppressed: false,
    };
    common.severity = crate::domain::Severity::Medium;
    let arch = vec![ArchitectureFinding { common }];
    let out = render_architecture_chunk(&arch);
    assert!(
        out.contains("100%25 bad%0Aline2"),
        "message must escape % and LF; got: {out}"
    );
    assert!(
        out.contains("file=src/foo%2Cwith%2Ccomma.rs"),
        "property must escape commas; got: {out}"
    );
    assert!(
        !out.contains("\nline2"),
        "no raw LF must remain in the annotation; got: {out}"
    );
}

#[test]
fn github_reporter_emits_orphan_annotations_via_snapshot_view() {
    use crate::domain::findings::OrphanSuppression;
    use crate::ports::Reporter;
    let summary = Summary {
        total: 1,
        quality_score: 1.0,
        ..Default::default()
    };
    let mut findings = crate::domain::AnalysisFindings::default();
    findings.orphan_suppressions = vec![OrphanSuppression {
        file: "src/foo.rs".into(),
        line: 42,
        dimensions: vec![crate::findings::Dimension::Srp],
        reason: Some("legacy".into()),
        kind: crate::domain::findings::OrphanKind::Stale,
    }];
    let reporter = GithubReporter { summary: &summary };
    let out = reporter.render(&findings, &crate::domain::AnalysisData::default());
    assert!(
        out.contains("file=src/foo.rs") && out.contains("line=42"),
        "orphan annotation must reach output via snapshot.orphans; got: {out}"
    );
}
