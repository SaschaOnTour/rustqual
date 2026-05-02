//! HTML rendering for the orphan-suppressions section.

use super::html_escape;
use crate::report::OrphanSuppressionWarning;

pub(super) fn format_orphan_suppressions_section(orphans: &[OrphanSuppressionWarning]) -> String {
    if orphans.is_empty() {
        return String::new();
    }
    let mut html = String::from(
        "<details>\n<summary>Orphan Suppressions</summary>\n\
         <div class=\"detail-content\">\n\
         <table>\n<thead><tr>\
         <th>File</th><th>Line</th><th>Scope</th><th>Reason</th>\
         </tr></thead>\n<tbody>\n",
    );
    orphans.iter().for_each(|w| html.push_str(&render_row(w)));
    html.push_str("</tbody></table>\n</div>\n</details>\n\n");
    html
}

fn render_row(w: &OrphanSuppressionWarning) -> String {
    let scope = if w.dimensions.is_empty() {
        "&lt;all&gt;".to_string()
    } else {
        w.dimensions
            .iter()
            .map(|d| html_escape(&d.to_string()))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let reason = w.reason.as_deref().map(html_escape).unwrap_or_default();
    format!(
        "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
        html_escape(&w.file),
        w.line,
        scope,
        reason,
    )
}
