mod architecture_table;
mod complexity;
mod coupling;
mod dry;
mod iosp;
mod orphan_suppressions;
mod srp_tables;
mod structural_table;
mod tq_table;
mod views;

use architecture_table::{build_architecture_view, format_architecture_section};
use complexity::{build_complexity_data_view, build_complexity_view, format_complexity_section};
use coupling::{build_coupling_data_view, build_coupling_view, format_coupling_section};
use dry::{build_dry_view, format_dry_section};
use iosp::{build_iosp_data_view, build_iosp_view, format_iosp_section};
use orphan_suppressions::format_orphan_suppressions_section;
use srp_tables::{build_srp_view, format_srp_section};
use structural_table::format_structural_section;
use tq_table::{build_tq_view, format_tq_section};

use views::{
    HtmlArchitectureView, HtmlComplexityDataView, HtmlComplexityView, HtmlCouplingDataView,
    HtmlCouplingView, HtmlDryView, HtmlIospDataView, HtmlIospView, HtmlSrpView, HtmlTqView,
};

use super::{AnalysisResult, Summary};
use crate::domain::analysis_data::{FunctionRecord, ModuleCouplingRecord};
use crate::domain::findings::{
    ArchitectureFinding, ComplexityFinding, CouplingFinding, DryFinding, IospFinding,
    OrphanSuppression, SrpFinding, TqFinding,
};
use crate::domain::PERCENTAGE_MULTIPLIER;
use crate::ports::reporter::{ReporterImpl, Snapshot};
use crate::ports::Reporter;

/// Escape HTML-special characters in user content.
/// Operation: string replacement logic.
pub(crate) fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// HTML reporter — produces a self-contained HTML report. Holds
/// `&Summary` for the dashboard composition; per-dim views handle
/// their respective collapsible sections, including orphan
/// suppressions which flow through the trait via `build_orphans` →
/// `Snapshot::orphans` → `publish`.
pub struct HtmlReporter<'a> {
    pub(crate) summary: &'a Summary,
}

impl<'a> ReporterImpl for HtmlReporter<'a> {
    type Output = String;

    type IospView = HtmlIospView;
    type ComplexityView = HtmlComplexityView;
    type DryView = HtmlDryView;
    type SrpView = HtmlSrpView;
    type CouplingView = HtmlCouplingView;
    type TestQualityView = HtmlTqView;
    type ArchitectureView = HtmlArchitectureView;
    type OrphanView = String;
    type IospDataView = HtmlIospDataView;
    type ComplexityDataView = HtmlComplexityDataView;
    type CouplingDataView = HtmlCouplingDataView;

    fn build_iosp(&self, findings: &[IospFinding]) -> HtmlIospView {
        build_iosp_view(findings)
    }
    fn build_complexity(&self, findings: &[ComplexityFinding]) -> HtmlComplexityView {
        build_complexity_view(findings)
    }
    fn build_dry(&self, findings: &[DryFinding]) -> HtmlDryView {
        build_dry_view(findings)
    }
    fn build_srp(&self, findings: &[SrpFinding]) -> HtmlSrpView {
        build_srp_view(findings)
    }
    fn build_coupling(&self, findings: &[CouplingFinding]) -> HtmlCouplingView {
        build_coupling_view(findings)
    }
    fn build_test_quality(&self, findings: &[TqFinding]) -> HtmlTqView {
        build_tq_view(findings)
    }
    fn build_architecture(&self, findings: &[ArchitectureFinding]) -> HtmlArchitectureView {
        build_architecture_view(findings)
    }
    fn build_iosp_data(&self, fns: &[FunctionRecord]) -> HtmlIospDataView {
        build_iosp_data_view(fns)
    }
    fn build_complexity_data(&self, fns: &[FunctionRecord]) -> HtmlComplexityDataView {
        build_complexity_data_view(fns)
    }
    fn build_coupling_data(&self, modules: &[ModuleCouplingRecord]) -> HtmlCouplingDataView {
        build_coupling_data_view(modules)
    }
    fn build_orphans(&self, suppressions: &[OrphanSuppression]) -> String {
        format_orphan_suppressions_section(suppressions)
    }

    fn publish(&self, snapshot: Snapshot<Self>) -> String {
        let Snapshot {
            iosp,
            complexity,
            dry,
            srp,
            coupling,
            test_quality,
            architecture,
            orphans,
            iosp_data,
            complexity_data,
            coupling_data,
        } = snapshot;
        let mut html = String::with_capacity(HTML_INITIAL_CAPACITY);
        html.push_str(&html_header());
        html.push_str(&html_dashboard(self.summary));
        html.push_str(&format_iosp_section(&iosp, &iosp_data, self.summary));
        html.push_str(&format_complexity_section(&complexity, &complexity_data));
        html.push_str(&format_dry_section(&dry));
        html.push_str(&format_srp_section(&srp));
        html.push_str(&format_tq_section(&test_quality));
        html.push_str(&format_structural_section(
            &srp.structural_rows,
            &coupling.structural_rows,
        ));
        html.push_str(&format_coupling_section(&coupling, &coupling_data));
        html.push_str(&format_architecture_section(&architecture));
        html.push_str(&orphans);
        html.push_str(&html_footer());
        html
    }
}

/// Print the analysis results as a self-contained HTML report.
/// Trivial: render via the Reporter trait + println.
pub fn print_html(analysis: &AnalysisResult) {
    let reporter = HtmlReporter {
        summary: &analysis.summary,
    };
    println!("{}", reporter.render(&analysis.findings, &analysis.data));
}

/// Initial capacity for the HTML output buffer.
const HTML_INITIAL_CAPACITY: usize = 32768;

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
        "Architecture",
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
mod tests;
