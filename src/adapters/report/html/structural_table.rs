//! HTML Structural Checks section: cross-dim section combining the
//! `Structural`-variant rows from both SRP and Coupling views.

use super::html_escape;
use super::views::HtmlStructuralRow;

/// Build the cross-dim Structural section. Empty input → empty section.
pub(super) fn format_structural_section(
    srp_rows: &[HtmlStructuralRow],
    coupling_rows: &[HtmlStructuralRow],
) -> String {
    let total = srp_rows.len() + coupling_rows.len();
    super::html_section_wrapper(
        "Structural Checks",
        total,
        "No structural warnings.",
        || format_structural_table(srp_rows, coupling_rows),
    )
}

fn format_structural_table(
    srp_rows: &[HtmlStructuralRow],
    coupling_rows: &[HtmlStructuralRow],
) -> String {
    if srp_rows.is_empty() && coupling_rows.is_empty() {
        return String::new();
    }
    let esc = |s: &str| html_escape(s);
    let mut html = String::from(
        "<table>\n<thead><tr>\
         <th>Code</th><th>Name</th><th>File</th><th>Line</th>\
         <th>Dimension</th><th>Detail</th>\
         </tr></thead>\n<tbody>\n",
    );
    srp_rows.iter().for_each(|r| {
        html.push_str(&format_row(r, "SRP", &esc));
    });
    coupling_rows.iter().for_each(|r| {
        html.push_str(&format_row(r, "Coupling", &esc));
    });
    html.push_str("</tbody></table>\n");
    html
}

fn format_row(r: &HtmlStructuralRow, dim: &str, esc: &dyn Fn(&str) -> String) -> String {
    format!(
        "<tr><td><span class=\"tag tag-warning\">{}</span></td>\
         <td>{}</td><td>{}</td><td>{}</td>\
         <td>{}</td><td>{}</td></tr>\n",
        esc(&r.code),
        esc(&r.name),
        esc(&r.file),
        r.line,
        dim,
        esc(&r.detail),
    )
}
