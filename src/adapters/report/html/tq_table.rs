//! HTML Test-Quality section.

use super::html_escape;
use super::views::HtmlTqView;
use crate::adapters::report::projections::tq::{project_tq_rows, TqRow};
use crate::domain::findings::TqFinding;

pub(super) fn build_tq_view(findings: &[TqFinding]) -> HtmlTqView {
    HtmlTqView {
        warnings: project_tq_rows(findings),
    }
}

pub(super) fn format_tq_section(view: &HtmlTqView) -> String {
    let count = view.warnings.len();
    super::html_section_wrapper("Test Quality", count, "No test quality warnings.", || {
        format_tq_table(view)
    })
}

fn format_tq_table(view: &HtmlTqView) -> String {
    if view.warnings.is_empty() {
        return String::new();
    }
    let mut html = String::from(
        "<table>\n<thead><tr>\
         <th>Function</th><th>File</th><th>Line</th>\
         <th>Kind</th><th>Detail</th>\
         </tr></thead>\n<tbody>\n",
    );
    view.warnings.iter().for_each(|w| {
        html.push_str(&format_tq_row(w));
    });
    html.push_str("</tbody></table>\n");
    html
}

fn format_tq_row(w: &TqRow) -> String {
    format!(
        "<tr><td>{}</td><td>{}</td><td>{}</td>\
         <td><span class=\"tag tag-warning\">{}</span></td>\
         <td>{}</td></tr>\n",
        html_escape(&w.function_name),
        html_escape(&w.file),
        w.line,
        w.display_label,
        html_escape(&w.detail),
    )
}
