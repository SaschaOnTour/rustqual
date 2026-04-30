//! HTML Complexity section: per-function complexity metrics + warning
//! issues. Joins the flagged-keys View (from findings) with the
//! function rows (from data) — a function is rendered when it both
//! has metrics over threshold (data) and a matching finding (lookup).

use super::html_escape;
use super::views::{
    HtmlComplexityDataView, HtmlComplexityFunctionRow, HtmlComplexityKey, HtmlComplexityView,
};
use crate::domain::analysis_data::FunctionRecord;
use crate::domain::findings::{ComplexityFinding, ComplexityFindingKind};

/// Project ComplexityFindings into a list of (file, line, kind) lookup
/// keys. Magic-number findings flag the whole file regardless of line.
pub(super) fn build_complexity_view(findings: &[ComplexityFinding]) -> HtmlComplexityView {
    let flagged_keys = findings
        .iter()
        .filter(|f| !f.common.suppressed)
        .map(|f| HtmlComplexityKey {
            file: f.common.file.clone(),
            line: f.common.line,
            is_magic_number: matches!(f.kind, ComplexityFindingKind::MagicNumber),
        })
        .collect();
    HtmlComplexityView { flagged_keys }
}

/// Project function records into the complexity data view with
/// pre-computed metrics + issue summary string.
pub(super) fn build_complexity_data_view(functions: &[FunctionRecord]) -> HtmlComplexityDataView {
    let functions = functions
        .iter()
        .map(|f| {
            let metrics = f.complexity.as_ref();
            let issue_summary = build_issue_summary(metrics);
            HtmlComplexityFunctionRow {
                qualified_name: f.qualified_name.clone(),
                file: f.file.clone(),
                line: f.line,
                cognitive: metrics.map(|m| m.cognitive_complexity).unwrap_or(0),
                cyclomatic: metrics.map(|m| m.cyclomatic_complexity).unwrap_or(0),
                max_nesting: metrics.map(|m| m.max_nesting).unwrap_or(0),
                function_lines: metrics.map(|m| m.function_lines).unwrap_or(0),
                issue_summary,
                suppressed: f.suppressed,
                complexity_suppressed: f.complexity_suppressed,
            }
        })
        .collect();
    HtmlComplexityDataView { functions }
}

fn build_issue_summary(
    metrics: Option<&crate::domain::analysis_data::ComplexityMetricsRecord>,
) -> String {
    let magic_issue = metrics.filter(|m| !m.magic_numbers.is_empty()).map(|m| {
        let mn: Vec<String> = m
            .magic_numbers
            .iter()
            .map(|n| format!("{} (line {})", n.value, n.line))
            .collect();
        format!("magic: {}", mn.join(", "))
    });
    let unsafe_issue =
        metrics.and_then(|m| (m.unsafe_blocks > 0).then(|| format!("{} unsafe", m.unsafe_blocks)));
    let err_issue = metrics.and_then(|m| {
        let parts: Vec<String> = [
            (m.unwrap_count, "unwrap"),
            (m.expect_count, "expect"),
            (m.panic_count, "panic"),
            (m.todo_count, "todo"),
        ]
        .iter()
        .filter(|(c, _)| *c > 0)
        .map(|(c, l)| format!("{c}{l}"))
        .collect();
        (!parts.is_empty()).then(|| parts.join(", "))
    });
    let issues: Vec<&str> = [&magic_issue, &unsafe_issue, &err_issue]
        .iter()
        .filter_map(|o| o.as_ref().map(|s| s.as_str()))
        .collect();
    if issues.is_empty() {
        "\u{2014}".to_string()
    } else {
        issues.join("; ")
    }
}

/// Format the complexity section. Filters data rows by membership in
/// the finding-side flagged-keys lookup.
pub(super) fn format_complexity_section(
    finding_view: &HtmlComplexityView,
    data_view: &HtmlComplexityDataView,
) -> String {
    let warnings: Vec<&HtmlComplexityFunctionRow> = data_view
        .functions
        .iter()
        .filter(|f| !f.suppressed && !f.complexity_suppressed)
        .filter(|f| matches_any_flagged(&finding_view.flagged_keys, f))
        .collect();
    let mut html = String::new();
    html.push_str(&format!(
        "<details>\n<summary>Complexity \u{2014} {} Warning{}</summary>\n\
         <div class=\"detail-content\">\n",
        warnings.len(),
        if warnings.len() == 1 { "" } else { "s" },
    ));
    if warnings.is_empty() {
        html.push_str("<p class=\"empty-state\">No complexity warnings.</p>\n");
    } else {
        html.push_str(
            "<table>\n<thead><tr><th>Function</th><th>File</th><th>Line</th>\
             <th>Cognitive</th><th>Cyclomatic</th><th>Nesting</th>\
             <th>Lines</th><th>Issues</th></tr></thead>\n<tbody>\n",
        );
        warnings.iter().for_each(|f| {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>\
                 <td>{}</td><td>{}</td><td>{}</td></tr>\n",
                html_escape(&f.qualified_name),
                html_escape(&f.file),
                f.line,
                f.cognitive,
                f.cyclomatic,
                f.max_nesting,
                f.function_lines,
                html_escape(&f.issue_summary),
            ));
        });
        html.push_str("</tbody></table>\n");
    }
    html.push_str("</div>\n</details>\n\n");
    html
}

fn matches_any_flagged(keys: &[HtmlComplexityKey], row: &HtmlComplexityFunctionRow) -> bool {
    keys.iter()
        .any(|k| k.file == row.file && (k.line == row.line || k.is_magic_number))
}
