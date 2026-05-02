use crate::adapters::analyzers::iosp::{
    compute_severity, CallOccurrence, Classification, ComplexityMetrics, FunctionAnalysis,
    LogicOccurrence, MagicNumberOccurrence,
};
use crate::ports::Reporter;
use crate::report::html::*;
use crate::report::Summary;

/// Test-local helper: instantiate the reporter and render. Replaces
/// the production `build_html_string` shim (whose only consumer was
/// these tests).
fn build_html_string(analysis: &AnalysisResult) -> String {
    HtmlReporter {
        summary: &analysis.summary,
        orphan_suppressions: &analysis.orphan_suppressions,
    }
    .render(&analysis.findings, &analysis.data)
}

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
    let mut summary = Summary::from_results(&results);
    summary.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
    let config = crate::config::Config::default();
    let data = crate::app::projection::project_data(&results, None);
    let findings = crate::domain::AnalysisFindings {
        iosp: crate::app::projection::project_iosp(&results),
        complexity: crate::app::projection::project_complexity(&results, &config),
        ..Default::default()
    };
    AnalysisResult {
        results,
        summary,
        orphan_suppressions: vec![],
        findings,
        data,
    }
}

#[test]
fn test_html_contains_doctype() {
    let analysis = make_analysis(vec![make_result("f", Classification::Operation)]);
    let html = build_html_string(&analysis);
    assert!(html.starts_with("<!DOCTYPE html>"));
}

#[test]
fn test_html_contains_style() {
    let analysis = make_analysis(vec![make_result("f", Classification::Operation)]);
    let html = build_html_string(&analysis);
    assert!(html.contains("<style>"));
    assert!(html.contains("</style>"));
}

#[test]
fn test_html_contains_dashboard() {
    let analysis = make_analysis(vec![make_result("f", Classification::Operation)]);
    let html = build_html_string(&analysis);
    assert!(html.contains("class=\"dashboard\""));
    assert!(html.contains("Quality Score:"));
}

#[test]
fn test_html_quality_score_displayed() {
    let analysis = make_analysis(vec![make_result("f", Classification::Operation)]);
    let html = build_html_string(&analysis);
    assert!(html.contains("100.0%"));
}

#[test]
fn test_html_iosp_section_present() {
    let analysis = make_analysis(vec![make_result("f", Classification::Operation)]);
    let html = build_html_string(&analysis);
    assert!(html.contains("IOSP"));
    assert!(html.contains("No IOSP violations."));
}

#[test]
fn test_html_no_violations_message() {
    let analysis = make_analysis(vec![make_result("f", Classification::Integration)]);
    let html = build_html_string(&analysis);
    assert!(html.contains("No IOSP violations."));
}

#[test]
fn test_html_with_violations_table() {
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
    let html = build_html_string(&analysis);
    assert!(html.contains("bad_fn"));
    assert!(html.contains("<table>"));
    assert!(html.contains("helper"));
}

#[test]
fn test_html_complexity_section() {
    let mut func = make_result("complex_fn", Classification::Operation);
    func.complexity = Some(ComplexityMetrics {
        logic_count: 5,
        call_count: 0,
        max_nesting: 3,
        cognitive_complexity: 20,
        cyclomatic_complexity: 12,
        magic_numbers: vec![MagicNumberOccurrence {
            line: 10,
            value: "42".to_string(),
        }],
        ..Default::default()
    });
    func.cognitive_warning = true;
    let analysis = make_analysis(vec![func]);
    let html = build_html_string(&analysis);
    assert!(html.contains("complex_fn"));
    assert!(html.contains("42"));
}

#[test]
fn test_html_dry_section_empty() {
    let analysis = make_analysis(vec![make_result("f", Classification::Operation)]);
    let html = build_html_string(&analysis);
    assert!(html.contains("No DRY issues found."));
}

#[test]
fn test_html_srp_section_empty() {
    let analysis = make_analysis(vec![make_result("f", Classification::Operation)]);
    let html = build_html_string(&analysis);
    assert!(html.contains("No SRP warnings."));
}

#[test]
fn test_html_coupling_section_empty() {
    let analysis = make_analysis(vec![make_result("f", Classification::Operation)]);
    let html = build_html_string(&analysis);
    assert!(html.contains("No coupling data."));
}

#[test]
fn test_html_self_contained_no_external() {
    let analysis = make_analysis(vec![make_result("f", Classification::Operation)]);
    let html = build_html_string(&analysis);
    // No external resource references
    assert!(!html.contains("http://"));
    assert!(!html.contains("https://"));
    assert!(!html.contains("<link"));
    assert!(!html.contains("<script src"));
}

#[test]
fn test_html_empty_results() {
    let analysis = make_analysis(vec![]);
    let html = build_html_string(&analysis);
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("</html>"));
}

#[test]
fn test_html_footer_closes_tags() {
    let analysis = make_analysis(vec![make_result("f", Classification::Operation)]);
    let html = build_html_string(&analysis);
    assert!(html.contains("</body>"));
    assert!(html.contains("</html>"));
    assert!(html.contains("rustqual"));
}

#[test]
fn test_html_escapes_special_chars() {
    let escaped = html_escape("<script>alert('xss')</script>");
    assert!(escaped.contains("&lt;"));
    assert!(escaped.contains("&gt;"));
    assert!(!escaped.contains("<script>"));
}

#[test]
fn test_html_renders_architecture_findings() {
    let mut analysis = make_analysis(vec![]);
    analysis.findings.architecture = vec![crate::domain::findings::ArchitectureFinding {
        common: crate::domain::Finding {
            file: "src/cli/handlers.rs".into(),
            line: 17,
            column: 0,
            dimension: crate::findings::Dimension::Architecture,
            rule_id: "architecture/call_parity/no_delegation".into(),
            message: "cli pub fn delegates to no application function".into(),
            severity: crate::domain::Severity::Medium,
            suppressed: false,
        },
    }];
    let html = build_html_string(&analysis);
    assert!(
        html.contains("architecture/call_parity/no_delegation"),
        "HTML must contain the architecture rule_id; got:\n{html}"
    );
    assert!(
        html.contains("src/cli/handlers.rs"),
        "HTML must contain the architecture finding file path"
    );
}
