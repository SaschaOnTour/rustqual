use crate::adapters::analyzers::structural::StructuralAnalysis;
use crate::report::html::html_escape;

/// Build the Structural Checks HTML section.
/// Trivial: single delegation to html_section_wrapper.
pub(super) fn html_structural_section(structural: Option<&StructuralAnalysis>) -> String {
    let count = structural
        .map(|s| s.warnings.iter().filter(|w| !w.suppressed).count())
        .unwrap_or(0);
    super::html_section_wrapper(
        "Structural Checks",
        count,
        "No structural warnings.",
        || html_structural_table(structural),
    )
}

/// Build HTML table rows for structural warnings.
/// Operation: iteration and formatting logic, no own calls (html_escape via closure).
fn html_structural_table(structural: Option<&StructuralAnalysis>) -> String {
    let warnings: Vec<_> = structural
        .map(|s| s.warnings.iter().filter(|w| !w.suppressed).collect())
        .unwrap_or_default();
    if warnings.is_empty() {
        return String::new();
    }
    let esc = |s: &str| html_escape(s);
    let mut html = String::from(
        "<table>\n<thead><tr>\
         <th>Code</th><th>Name</th><th>File</th><th>Line</th>\
         <th>Dimension</th><th>Detail</th>\
         </tr></thead>\n<tbody>\n",
    );
    warnings.iter().for_each(|w| {
        let code = w.kind.code();
        let detail = esc(&w.kind.detail());
        let dim_label = match w.dimension {
            crate::findings::Dimension::Srp => "SRP",
            crate::findings::Dimension::Coupling => "Coupling",
            _ => "SRP",
        };
        html.push_str(&format!(
            "<tr><td><span class=\"tag tag-warning\">{code}</span></td>\
             <td>{}</td><td>{}</td><td>{}</td>\
             <td>{dim_label}</td><td>{detail}</td></tr>\n",
            esc(&w.name),
            esc(&w.file),
            w.line,
        ));
    });
    html.push_str("</tbody></table>\n");
    html
}
