//! HTML rendering for the orphan-suppressions section.

use super::html_escape;
use crate::domain::findings::OrphanSuppression;

pub(super) fn format_orphan_suppressions_section(orphans: &[OrphanSuppression]) -> String {
    if orphans.is_empty() {
        return String::new();
    }
    let mut html = String::from(
        "<details>\n<summary>Orphan Suppressions</summary>\n\
         <div class=\"detail-content\">\n\
         <table>\n<thead><tr>\
         <th>File</th><th>Line</th><th>Status</th><th>Marker</th><th>Reason</th>\
         </tr></thead>\n<tbody>\n",
    );
    orphans.iter().for_each(|w| html.push_str(&render_row(w)));
    html.push_str("</tbody></table>\n</div>\n</details>\n\n");
    html
}

fn render_row(w: &OrphanSuppression) -> String {
    // Build the marker from RAW dimensions, then escape once — escaping the
    // parts first would double-escape them inside the assembled spec.
    let dims = if w.dimensions.is_empty() {
        "<all>".to_string()
    } else {
        w.dimensions
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let scope = html_escape(&w.marker_spec(&dims));
    let reason = w.reason.as_deref().map(html_escape).unwrap_or_default();
    format!(
        "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
        html_escape(&w.file),
        w.line,
        w.kind.status_word(),
        scope,
        reason,
    )
}
