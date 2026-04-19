mod sections;
mod srp_tables;
mod structural_table;
mod tables;
mod tq_table;

use sections::{html_complexity_section, html_coupling_section, html_iosp_section};
use structural_table::html_structural_section;
use tables::{html_dry_section, html_srp_section, html_tq_section};

use super::{AnalysisResult, Summary};
use crate::adapters::analyzers::iosp::PERCENTAGE_MULTIPLIER;

/// Escape HTML-special characters in user content.
/// Operation: string replacement logic.
pub(crate) fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Print the analysis results as a self-contained HTML report.
/// Trivial: single call to build_html_string + println.
pub fn print_html(analysis: &AnalysisResult) {
    let html = build_html_string(analysis);
    println!("{html}");
}

/// Initial capacity for the HTML output buffer.
const HTML_INITIAL_CAPACITY: usize = 32768;

/// Build the complete HTML string by assembling all sections.
/// Integration: calls section builders, no logic.
fn build_html_string(analysis: &AnalysisResult) -> String {
    let mut html = String::with_capacity(HTML_INITIAL_CAPACITY);
    html.push_str(&html_header());
    html.push_str(&html_dashboard(&analysis.summary));
    html.push_str(&html_iosp_section(&analysis.results, &analysis.summary));
    html.push_str(&html_complexity_section(&analysis.results));
    html.push_str(&html_dry_section(analysis));
    html.push_str(&html_srp_section(analysis.srp.as_ref()));
    html.push_str(&html_tq_section(analysis.tq.as_ref()));
    html.push_str(&html_structural_section(analysis.structural.as_ref()));
    html.push_str(&html_coupling_section(analysis.coupling.as_ref()));
    html.push_str(&html_footer());
    html
}

/// Build the HTML header with DOCTYPE, meta tags, and all CSS.
/// Operation: string building, no own calls.
fn html_header() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>rustqual Analysis Report</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;background:#f7fafc;color:#2d3748;line-height:1.6;padding:2rem;max-width:1200px;margin:0 auto}
h1{font-size:1.8rem;margin-bottom:.25rem}
h2{font-size:1.3rem;margin:1.5rem 0 .75rem;border-bottom:2px solid #e2e8f0;padding-bottom:.25rem}
h3{font-size:1.05rem;margin:1rem 0 .5rem;color:#4a5568}
.score-badge{display:inline-block;padding:.25rem .75rem;border-radius:9999px;font-weight:700;font-size:1.4rem;color:white;margin:.5rem 0}
.dashboard{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:1rem;margin:1.5rem 0}
.card{background:white;border-radius:8px;padding:1rem;box-shadow:0 1px 3px rgba(0,0,0,.1);text-align:center}
.card .label{font-size:.85rem;color:#718096;text-transform:uppercase;letter-spacing:.05em}
.card .value{font-size:1.8rem;font-weight:700;margin:.25rem 0}
.stats{display:grid;grid-template-columns:repeat(auto-fit,minmax(140px,1fr));gap:.75rem;margin:1rem 0}
.stat{background:white;border-radius:6px;padding:.75rem;box-shadow:0 1px 2px rgba(0,0,0,.06);text-align:center}
.stat .label{font-size:.75rem;color:#a0aec0;text-transform:uppercase}
.stat .value{font-size:1.2rem;font-weight:600}
details{background:white;border-radius:8px;margin:1rem 0;box-shadow:0 1px 3px rgba(0,0,0,.1)}
summary{padding:.75rem 1rem;cursor:pointer;font-weight:600;user-select:none}
summary:hover{background:#f7fafc;border-radius:8px}
.detail-content{padding:0 1rem 1rem}
table{width:100%;border-collapse:collapse;font-size:.9rem}
th{text-align:left;padding:.5rem;border-bottom:2px solid #e2e8f0;color:#718096;font-size:.8rem;text-transform:uppercase}
td{padding:.5rem;border-bottom:1px solid #edf2f7}
tr:hover{background:#f7fafc}
.severity-high{color:#e53e3e;font-weight:600}
.severity-medium{color:#dd6b20}
.severity-low{color:#718096}
.tag{display:inline-block;padding:.1rem .5rem;border-radius:4px;font-size:.8rem;font-weight:500}
.tag-violation{background:#fed7d7;color:#c53030}
.tag-warning{background:#fefcbf;color:#975a16}
.tag-ok{background:#c6f6d5;color:#276749}
.empty-state{padding:2rem;text-align:center;color:#a0aec0;font-style:italic}
footer{margin-top:2rem;padding-top:1rem;border-top:1px solid #e2e8f0;font-size:.8rem;color:#a0aec0;text-align:center}
</style>
</head>
<body>
"#
    .to_string()
}

/// Build the HTML dashboard with quality score and dimension cards.
/// Operation: formatting logic with closures for color coding.
fn html_dashboard(summary: &Summary) -> String {
    let pct = |v: f64| v * PERCENTAGE_MULTIPLIER;
    let color = |s: f64| -> &str {
        if s >= 0.8 {
            "#48bb78"
        } else if s >= 0.5 {
            "#ecc94b"
        } else {
            "#f56565"
        }
    };

    let names = [
        "IOSP",
        "Complexity",
        "DRY",
        "SRP",
        "Coupling",
        "Test Quality",
    ];
    let scores = &summary.dimension_scores;
    let q = summary.quality_score;

    let mut html = String::new();
    html.push_str("<header>\n");
    html.push_str("  <h1>rustqual Analysis Report</h1>\n");
    html.push_str(&format!(
        "  <span class=\"score-badge\" style=\"background:{}\">\
         Quality Score: {:.1}%</span>\n",
        color(q),
        pct(q)
    ));
    html.push_str("</header>\n\n<section class=\"dashboard\">\n");

    names.iter().enumerate().for_each(|(i, name)| {
        html.push_str(&format!(
            "  <div class=\"card\">\
             <div class=\"label\">{name}</div>\
             <div class=\"value\" style=\"color:{}\">{:.1}%</div>\
             </div>\n",
            color(scores[i]),
            pct(scores[i])
        ));
    });
    html.push_str("</section>\n\n");

    // Summary stats row
    html.push_str("<section class=\"stats\">\n");
    html.push_str(&format!(
        "  <div class=\"stat\"><div class=\"label\">Functions</div>\
         <div class=\"value\">{}</div></div>\n",
        summary.total
    ));
    html.push_str(&format!(
        "  <div class=\"stat\"><div class=\"label\">Violations</div>\
         <div class=\"value\" style=\"color:{}\">{}</div></div>\n",
        if summary.violations > 0 {
            "#e53e3e"
        } else {
            "#48bb78"
        },
        summary.violations
    ));
    html.push_str(&format!(
        "  <div class=\"stat\"><div class=\"label\">Findings</div>\
         <div class=\"value\" style=\"color:{}\">{}</div></div>\n",
        if summary.total_findings() > 0 {
            "#dd6b20"
        } else {
            "#48bb78"
        },
        summary.total_findings()
    ));
    html.push_str(&format!(
        "  <div class=\"stat\"><div class=\"label\">All Allows</div>\
         <div class=\"value\">{}{}</div></div>\n",
        summary.all_suppressions,
        if summary.suppression_ratio_exceeded {
            " <span class=\"tag tag-warning\">ratio exceeded</span>"
        } else {
            ""
        },
    ));
    html.push_str("</section>\n\n");
    html
}

/// Build a complete HTML collapsible section: header + body from closure + footer.
/// Operation: formatting logic, calls table_builder via closure parameter.
pub(super) fn html_section_wrapper(
    title: &str,
    count: usize,
    empty_msg: &str,
    table_builder: impl FnOnce() -> String,
) -> String {
    let mut html = String::new();
    html.push_str(&format!(
        "<details>\n<summary>{title} \u{2014} {} Warning{}</summary>\n\
         <div class=\"detail-content\">\n",
        count,
        if count == 1 { "" } else { "s" },
    ));
    if count == 0 {
        html.push_str(&format!("<p class=\"empty-state\">{empty_msg}</p>\n"));
    }
    html.push_str(&table_builder());
    html.push_str("</div>\n</details>\n\n");
    html
}

/// Build the HTML footer.
/// Trivial: static string.
fn html_footer() -> String {
    "<footer>Generated by <strong>rustqual</strong></footer>\n</body>\n</html>\n".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::analyzers::iosp::{
        compute_severity, CallOccurrence, Classification, ComplexityMetrics, FunctionAnalysis,
        LogicOccurrence, MagicNumberOccurrence,
    };
    use crate::report::Summary;

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
        AnalysisResult {
            results,
            summary,
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
}
