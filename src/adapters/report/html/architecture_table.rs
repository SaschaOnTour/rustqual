//! HTML rendering for the Architecture dimension.
//!
//! `build_architecture_view` projects findings into the typed view;
//! `format_architecture_section` consumes the view and renders HTML.

use super::html_escape;
use super::views::{HtmlArchitectureRow, HtmlArchitectureView};
use crate::domain::findings::ArchitectureFinding;

/// Project findings into the typed view (only non-suppressed).
pub(super) fn build_architecture_view(findings: &[ArchitectureFinding]) -> HtmlArchitectureView {
    let rows = findings
        .iter()
        .filter(|f| !f.common.suppressed)
        .map(|f| HtmlArchitectureRow {
            rule_id: f.common.rule_id.clone(),
            file: f.common.file.clone(),
            line: f.common.line,
            message: f.common.message.clone(),
        })
        .collect();
    HtmlArchitectureView { findings: rows }
}

/// Build the Architecture section: a single table of all architecture
/// findings, or an empty-state stanza when there are none.
pub(super) fn format_architecture_section(view: &HtmlArchitectureView) -> String {
    let count = view.findings.len();
    let mut html = String::new();
    html.push_str(&format!(
        "<details{}>\n<summary>Architecture \u{2014} {count} Finding{}</summary>\n\
         <div class=\"detail-content\">\n",
        if count > 0 { " open" } else { "" },
        if count == 1 { "" } else { "s" },
    ));
    if count == 0 {
        html.push_str("<p class=\"empty-state\">No architecture findings.</p>\n");
        html.push_str("</div>\n</details>\n\n");
        return html;
    }
    html.push_str(
        "<table>\n<thead><tr><th>Rule</th><th>Location</th><th>Message</th></tr></thead>\n<tbody>\n",
    );
    view.findings.iter().for_each(|r| {
        html.push_str(&format!(
            "<tr><td><code>{}</code></td><td>{}:{}</td><td>{}</td></tr>\n",
            html_escape(&r.rule_id),
            html_escape(&r.file),
            r.line,
            html_escape(&r.message),
        ));
    });
    html.push_str("</tbody></table>\n");
    html.push_str("</div>\n</details>\n\n");
    html
}
