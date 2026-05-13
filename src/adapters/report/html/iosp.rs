//! HTML IOSP section: violation table per function. The table joins
//! `IospFinding` (logic + call locations) with `FunctionRecord` (name +
//! severity + effort). build_iosp / build_iosp_data project each side
//! separately; `format_iosp_section` does the join in the renderer.

use super::html_escape;
use super::views::{HtmlIospDataView, HtmlIospFindingRow, HtmlIospFunctionRow, HtmlIospView};
use crate::domain::analysis_data::{FunctionClassification, FunctionRecord};
use crate::domain::findings::IospFinding;
use crate::domain::{Severity, PERCENTAGE_MULTIPLIER};
use crate::report::Summary;

/// Project IOSP findings into the typed view.
pub(super) fn build_iosp_view(findings: &[IospFinding]) -> HtmlIospView {
    let rows = findings
        .iter()
        .filter(|f| !f.common.suppressed)
        .map(|f| HtmlIospFindingRow {
            file: f.common.file.clone(),
            line: f.common.line,
            logic_summary: f
                .logic_locations
                .iter()
                .map(|l| format!("{} (line {})", l.kind, l.line))
                .collect::<Vec<_>>()
                .join(", "),
            call_summary: f
                .call_locations
                .iter()
                .map(|c| format!("{} (line {})", c.name, c.line))
                .collect::<Vec<_>>()
                .join(", "),
        })
        .collect();
    HtmlIospView { findings: rows }
}

/// Project function records into the violation-table data view.
pub(super) fn build_iosp_data_view(functions: &[FunctionRecord]) -> HtmlIospDataView {
    let violations = functions
        .iter()
        .filter(|f| !f.suppressed && f.classification == FunctionClassification::Violation)
        .map(|f| {
            let (sc, st) = match &f.severity {
                Some(Severity::High) => ("severity-high", "High"),
                Some(Severity::Medium) => ("severity-medium", "Medium"),
                Some(Severity::Low) => ("severity-low", "Low"),
                None => ("", "\u{2014}"),
            };
            HtmlIospFunctionRow {
                qualified_name: f.qualified_name.clone(),
                file: f.file.clone(),
                line: f.line,
                severity_class: sc,
                severity_label: st,
                effort: f
                    .effort_score
                    .map(|e| format!("{e:.1}"))
                    .unwrap_or_default(),
            }
        })
        .collect();
    HtmlIospDataView { violations }
}

/// Format the IOSP section. Joins finding rows to function rows by
/// (file, line); function rows are the iteration driver.
pub(super) fn format_iosp_section(
    finding_view: &HtmlIospView,
    data_view: &HtmlIospDataView,
    summary: &Summary,
) -> String {
    let esc = |s: &str| html_escape(s);
    let vc = summary.violations;
    let mut html = String::new();
    html.push_str(&format!(
        "<details{}>\n<summary>IOSP \u{2014} {} Violation{}, {:.1}% Score</summary>\n\
         <div class=\"detail-content\">\n",
        if vc > 0 { " open" } else { "" },
        vc,
        if vc == 1 { "" } else { "s" },
        summary.iosp_score * PERCENTAGE_MULTIPLIER,
    ));
    if vc == 0 {
        html.push_str("<p class=\"empty-state\">No IOSP violations.</p>\n");
    } else {
        html.push_str(
            "<table>\n<thead><tr><th>Function</th><th>File</th><th>Line</th>\
             <th>Severity</th><th>Effort</th><th>Logic</th><th>Calls</th></tr></thead>\n<tbody>\n",
        );
        data_view.violations.iter().for_each(|fr| {
            let finding = finding_view
                .findings
                .iter()
                .find(|f| f.file == fr.file && f.line == fr.line);
            let (logic, calls) = finding
                .map(|f| (f.logic_summary.as_str(), f.call_summary.as_str()))
                .unwrap_or(("", ""));
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td>\
                 <td class=\"{}\">{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                esc(&fr.qualified_name),
                esc(&fr.file),
                fr.line,
                fr.severity_class,
                fr.severity_label,
                fr.effort,
                esc(logic),
                esc(calls),
            ));
        });
        html.push_str("</tbody></table>\n");
    }
    html.push_str("</div>\n</details>\n\n");
    html
}
