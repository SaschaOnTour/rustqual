use super::*;

// ── Smoke tests via the public entry point ─────────────────────────

#[test]
fn github_smoke_annotations() {
    // End-to-end: a clean analysis emits a quality notice/error and no
    // warnings; a violation emits a warning/error annotation carrying the
    // file location. (label, results, predicate over the rendered output)
    type Pred = fn(&str) -> bool;
    let cases: Vec<(&str, Vec<FunctionAnalysis>, Pred)> = vec![
        (
            "clean analysis → quality notice/error, no warnings",
            vec![make_result("good_fn", Classification::Integration)],
            |o| (o.contains("::notice::") || o.contains("::error::")) && !o.contains("::warning::"),
        ),
        (
            "violation → warning/error annotation with file location",
            vec![make_result("bad_fn", violation("if", "helper"))],
            |o| (o.contains("::warning") || o.contains("::error")) && o.contains("file=test.rs"),
        ),
    ];
    for (label, results, pred) in cases {
        let out = render_github(&make_analysis(results));
        assert!(pred(&out), "case {label}: got {out}");
    }
}

// ── Content tests against the GithubReporter trait directly ────────
//
// These test the per-dimension trait methods. They're loose: they
// verify the general shape of the output (level prefix, file/line
// presence) without pinning exact wording, so message rewordings
// don't break them.

#[test]
fn iosp_finding_emits_warning_with_file_and_line() {
    use crate::domain::findings::{CallLocation, LogicLocation};
    let f = IospFinding {
        common: Finding {
            file: "src/lib.rs".into(),
            line: 42,
            column: 0,
            dimension: crate::findings::Dimension::Iosp,
            rule_id: "iosp/violation".into(),
            message: "ignored".into(),
            severity: crate::domain::Severity::Medium,
            suppressed: false,
        },
        logic_locations: vec![LogicLocation {
            kind: "if".into(),
            line: 44,
        }],
        call_locations: vec![CallLocation {
            name: "helper".into(),
            line: 50,
        }],
        effort_score: Some(2.5),
    };
    let out = render_iosp_chunk(&[f]);
    assert!(
        out.starts_with("::warning"),
        "expected ::warning prefix; got {out}"
    );
    assert!(out.contains("file=src/lib.rs"));
    assert!(out.contains("line=42"));
    assert!(out.contains("IOSP violation"));
    assert!(out.contains("if"));
    assert!(out.contains("helper"));
}

#[test]
fn iosp_suppressed_finding_skipped() {
    let f = IospFinding {
        common: Finding {
            file: "src/lib.rs".into(),
            line: 42,
            column: 0,
            dimension: crate::findings::Dimension::Iosp,
            rule_id: "iosp/violation".into(),
            message: "ignored".into(),
            severity: crate::domain::Severity::Medium,
            suppressed: true,
        },
        logic_locations: vec![],
        call_locations: vec![],
        effort_score: None,
    };
    let out = render_iosp_chunk(&[f]);
    assert!(out.is_empty(), "suppressed finding must produce no output");
}

/// An architecture finding at `src/foo.rs:17` with the given rule and severity.
fn arch_finding(rule_id: &str, severity: crate::domain::Severity) -> ArchitectureFinding {
    ArchitectureFinding {
        common: Finding {
            file: "src/foo.rs".into(),
            line: 17,
            column: 0,
            dimension: crate::findings::Dimension::Architecture,
            rule_id: rule_id.into(),
            message: "msg".into(),
            severity,
            suppressed: false,
        },
    }
}

#[test]
fn architecture_finding_severity_maps_to_level() {
    // Severity maps to the GitHub annotation level: High → ::error,
    // Low → ::notice. (label, rule_id, severity, expected_prefix)
    use crate::domain::Severity;
    let cases: &[(&str, &str, Severity, &str)] = &[
        (
            "high → ::error",
            "architecture/layer/violation",
            Severity::High,
            "::error",
        ),
        (
            "low → ::notice",
            "architecture/call_parity/multi_touchpoint",
            Severity::Low,
            "::notice",
        ),
    ];
    for (label, rule_id, severity, prefix) in cases {
        let out = render_architecture_chunk(&[arch_finding(rule_id, severity.clone())]);
        assert!(out.starts_with(prefix), "case {label}: got {out}");
        assert!(out.contains(rule_id), "case {label}: got {out}");
    }
}

#[test]
fn empty_findings_produce_empty_output() {
    assert!(render_iosp_chunk(&[]).is_empty());
    assert!(render_complexity_chunk(&[]).is_empty());
    assert!(render_dry_chunk(&[]).is_empty());
    assert!(render_srp_chunk(&[]).is_empty());
    assert!(render_coupling_chunk(&[]).is_empty());
    assert!(render_tq_chunk(&[]).is_empty());
    assert!(render_architecture_chunk(&[]).is_empty());
}

#[test]
fn summary_annotation_no_violations_emits_notice() {
    let summary = Summary {
        total: 100,
        quality_score: 1.0,
        ..Default::default()
    };
    let out = render_summary_annotation(&summary);
    assert!(out.contains("::notice"));
    assert!(out.contains("100.0%"));
}

#[test]
fn summary_annotation_with_violations_emits_error() {
    let summary = Summary {
        total: 100,
        violations: 3,
        quality_score: 0.95,
        ..Default::default()
    };
    let out = render_summary_annotation(&summary);
    assert!(out.contains("::error"));
    assert!(out.contains("3 finding"));
    assert!(out.contains("3 IOSP violation"));
}

#[test]
fn summary_annotation_with_only_architecture_findings_emits_error() {
    // Default-fail exits nonzero for any finding; the GitHub summary
    // must reflect that and not say "::notice" when an architecture
    // finding alone fails the run.
    let summary = Summary {
        total: 100,
        violations: 0,
        architecture_warnings: 2,
        quality_score: 0.99,
        ..Default::default()
    };
    let out = render_summary_annotation(&summary);
    assert!(
        out.contains("::error"),
        "non-IOSP findings must still produce ::error so the GitHub summary \
         agrees with the default-fail exit code; got: {out}"
    );
}

#[test]
fn summary_annotation_with_suppression_excess_adds_warning() {
    let summary = Summary {
        total: 100,
        suppressed: 50,
        suppression_ratio_exceeded: true,
        ..Default::default()
    };
    let out = render_summary_annotation(&summary);
    assert!(out.contains("::warning"));
    assert!(out.contains("Suppression ratio"));
}
