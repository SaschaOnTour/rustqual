use crate::adapters::analyzers::iosp::{compute_severity, CallOccurrence, LogicOccurrence};
use crate::adapters::analyzers::iosp::{Classification, FunctionAnalysis};
use crate::adapters::report::github::build::{
    build_architecture_view, build_complexity_view, build_coupling_view, build_dry_view,
    build_iosp_view, build_srp_view, build_tq_view,
};
use crate::adapters::report::github::format::{
    format_architecture, format_complexity, format_coupling, format_dry, format_iosp, format_srp,
    format_tq,
};

// Wrappers that preserve the test API: take a finding slice, return
// the formatted annotation block. They go through the new build →
// format pipeline so the tests exercise the real path.
fn render_iosp_chunk(findings: &[IospFinding]) -> String {
    format_iosp(&build_iosp_view(findings))
}
fn render_architecture_chunk(findings: &[ArchitectureFinding]) -> String {
    format_architecture(&build_architecture_view(findings))
}
fn render_complexity_chunk(findings: &[crate::domain::findings::ComplexityFinding]) -> String {
    format_complexity(&build_complexity_view(findings))
}
fn render_dry_chunk(findings: &[crate::domain::findings::DryFinding]) -> String {
    format_dry(&build_dry_view(findings))
}
fn render_srp_chunk(findings: &[crate::domain::findings::SrpFinding]) -> String {
    format_srp(&build_srp_view(findings))
}
fn render_coupling_chunk(findings: &[crate::domain::findings::CouplingFinding]) -> String {
    format_coupling(&build_coupling_view(findings))
}
fn render_tq_chunk(findings: &[crate::domain::findings::TqFinding]) -> String {
    format_tq(&build_tq_view(findings))
}
use crate::domain::findings::{ArchitectureFinding, IospFinding};
use crate::domain::Finding;
use crate::report::github::*;
use crate::report::{AnalysisResult, Summary};

fn make_result(name: &str, classification: Classification) -> FunctionAnalysis {
    let severity = compute_severity(&classification);
    FunctionAnalysis {
        name: name.to_string(),
        file: "test.rs".to_string(),
        line: 1,
        classification,
        parent_type: None,
        suppressed: false,
        complexity: None,
        qualified_name: name.to_string(),
        severity,
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

fn make_analysis(results: Vec<FunctionAnalysis>) -> AnalysisResult {
    let summary = Summary::from_results(&results);
    AnalysisResult {
        results,
        summary,
        orphan_suppressions: vec![],
        findings: crate::domain::AnalysisFindings::default(),
        data: crate::domain::AnalysisData::default(),
    }
}

// ── Smoke tests via the public entry point ─────────────────────────

#[test]
fn test_print_github_no_violations_no_panic() {
    let analysis = make_analysis(vec![make_result("good_fn", Classification::Integration)]);
    print_github(&analysis);
}

#[test]
fn test_print_github_with_violation_no_panic() {
    let analysis = make_analysis(vec![make_result(
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
    )]);
    print_github(&analysis);
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

#[test]
fn architecture_finding_emits_severity_mapped_level() {
    let high = ArchitectureFinding {
        common: Finding {
            file: "src/foo.rs".into(),
            line: 17,
            column: 0,
            dimension: crate::findings::Dimension::Architecture,
            rule_id: "architecture/layer/violation".into(),
            message: "layer skip".into(),
            severity: crate::domain::Severity::High,
            suppressed: false,
        },
    };
    let out = render_architecture_chunk(&[high]);
    assert!(
        out.starts_with("::error"),
        "High severity → ::error; got {out}"
    );
    assert!(out.contains("architecture/layer/violation"));
}

#[test]
fn architecture_finding_low_severity_emits_notice() {
    let low = ArchitectureFinding {
        common: Finding {
            file: "src/foo.rs".into(),
            line: 17,
            column: 0,
            dimension: crate::findings::Dimension::Architecture,
            rule_id: "architecture/call_parity/multi_touchpoint".into(),
            message: "multi".into(),
            severity: crate::domain::Severity::Low,
            suppressed: false,
        },
    };
    let out = render_architecture_chunk(&[low]);
    assert!(out.starts_with("::notice"));
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
    assert!(out.contains("3 violation"));
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

// ── new ReporterImpl interface ──────────────────────────────────

#[test]
fn test_github_render_includes_summary_annotation() {
    use crate::ports::Reporter;
    let summary = Summary {
        total: 100,
        quality_score: 1.0,
        ..Default::default()
    };
    let reporter = GithubReporter {
        summary: &summary,
        orphan_suppressions: &[],
    };
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
    let reporter = GithubReporter {
        summary: &summary,
        orphan_suppressions: &[],
    };
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
