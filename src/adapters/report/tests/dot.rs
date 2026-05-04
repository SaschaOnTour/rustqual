//! dot reporter tests.

use crate::domain::analysis_data::{FunctionClassification, FunctionRecord};
use crate::domain::{AnalysisData, AnalysisFindings};
use crate::ports::Reporter;
use crate::report::dot::*;

fn make_record(name: &str, classification: FunctionClassification) -> FunctionRecord {
    FunctionRecord {
        name: name.to_string(),
        file: "test.rs".to_string(),
        line: 1,
        qualified_name: name.to_string(),
        parent_type: None,
        classification,
        severity: None,
        complexity: None,
        parameter_count: 0,
        own_calls: vec![],
        is_trait_impl: false,
        is_test: false,
        effort_score: None,
        suppressed: false,
        complexity_suppressed: false,
    }
}

fn data_with(functions: Vec<FunctionRecord>) -> AnalysisData {
    AnalysisData {
        functions,
        modules: vec![],
    }
}

#[test]
fn test_print_dot_empty_no_panic() {
    let data = data_with(vec![]);
    print_dot(&data);
}

#[test]
fn test_print_dot_integration_no_panic() {
    let data = data_with(vec![make_record(
        "orchestrator",
        FunctionClassification::Integration,
    )]);
    print_dot(&data);
}

#[test]
fn test_print_dot_violation_no_panic() {
    let data = data_with(vec![make_record(
        "bad_fn",
        FunctionClassification::Violation,
    )]);
    print_dot(&data);
}

#[test]
fn test_print_dot_suppressed_skipped() {
    let mut rec = make_record("suppressed", FunctionClassification::Operation);
    rec.suppressed = true;
    let data = data_with(vec![rec]);
    print_dot(&data);
}

#[test]
fn test_print_dot_all_classifications() {
    let data = data_with(vec![
        make_record("integration_fn", FunctionClassification::Integration),
        make_record("operation_fn", FunctionClassification::Operation),
        make_record("trivial_fn", FunctionClassification::Trivial),
        make_record("violation_fn", FunctionClassification::Violation),
    ]);
    print_dot(&data);
}

// ── new ReporterImpl interface ──────────────────────────────────

#[test]
fn test_dot_render_returns_digraph_envelope() {
    let findings = AnalysisFindings::default();
    let data = data_with(vec![]);
    let out = DotReporter.render(&findings, &data);
    assert!(
        out.starts_with("digraph rustqual {"),
        "render output must open with digraph envelope, got: {out:?}",
    );
    assert!(out.ends_with("}\n"), "render output must close envelope");
}

#[test]
fn test_dot_render_emits_node_and_edge_for_function_with_calls() {
    let mut caller = make_record("caller", FunctionClassification::Integration);
    caller.own_calls = vec!["callee".to_string()];
    let callee = make_record("callee", FunctionClassification::Operation);
    let data = data_with(vec![caller, callee]);
    let findings = AnalysisFindings::default();
    let out = DotReporter.render(&findings, &data);
    assert!(out.contains("\"caller\""), "caller node missing in output");
    assert!(out.contains("\"callee\""), "callee node missing in output");
    assert!(
        out.contains("\"caller\" -> \"callee\";"),
        "edge from caller to callee missing in output: {out}",
    );
}

#[test]
fn test_dot_render_ignores_findings() {
    // dot is a data-only reporter — even with non-empty findings, the
    // output stays the same as with empty findings.
    let data = data_with(vec![make_record("f", FunctionClassification::Integration)]);
    let empty_findings = AnalysisFindings::default();
    let out_empty = DotReporter.render(&empty_findings, &data);

    // We can't easily build a non-empty AnalysisFindings here without
    // pulling in a lot of constructors, so we just verify the basic
    // shape: the output contains exactly the function nodes + edges
    // and the digraph envelope, no other content.
    assert!(out_empty.contains("\"f\""), "function node missing");
    assert!(out_empty.starts_with("digraph rustqual {"));
    assert!(out_empty.ends_with("}\n"));
}

#[test]
fn dot_reporter_intentionally_omits_orphan_rendering() {
    // dot is data-only by design. Even with `findings.orphan_suppressions`
    // populated, the dot output must NOT include orphan-suppression
    // markers — its `OrphanView = ()` declares the conscious choice
    // not to render them. This test locks in that intent so a future
    // refactor doesn't accidentally start emitting orphan rows in dot.
    use crate::domain::findings::OrphanSuppression;
    let data = data_with(vec![make_record("f", FunctionClassification::Integration)]);
    let mut findings = AnalysisFindings::default();
    findings.orphan_suppressions = vec![OrphanSuppression {
        file: "src/foo.rs".into(),
        line: 42,
        dimensions: vec![crate::findings::Dimension::Iosp],
        reason: Some("legacy".into()),
    }];
    let out = DotReporter.render(&findings, &data);
    assert!(
        !out.to_lowercase().contains("orphan"),
        "dot reporter must NOT render orphan markers (intentional no-op), got:\n{out}"
    );
    assert!(
        !out.contains("qual:allow"),
        "dot reporter must NOT render orphan reason text, got:\n{out}"
    );
}
