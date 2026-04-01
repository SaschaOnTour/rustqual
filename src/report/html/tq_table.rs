use crate::report::html::html_escape;

/// Build the Test Quality analysis section.
/// Trivial: single delegation to html_section_wrapper.
pub(super) fn html_tq_section(tq: Option<&crate::tq::TqAnalysis>) -> String {
    let count = tq
        .map(|t| t.warnings.iter().filter(|w| !w.suppressed).count())
        .unwrap_or(0);
    super::html_section_wrapper(
        "Test Quality",
        count,
        "No test quality warnings.",
        || html_tq_table(tq),
    )
}

/// Build HTML table rows for TQ warnings.
/// Operation: iteration and formatting logic, no own calls (html_escape via closure).
fn html_tq_table(tq: Option<&crate::tq::TqAnalysis>) -> String {
    let warnings: Vec<_> = tq
        .map(|t| t.warnings.iter().filter(|w| !w.suppressed).collect())
        .unwrap_or_default();
    if warnings.is_empty() {
        return String::new();
    }
    let esc = |s: &str| html_escape(s);
    let kind_label = |kind: &crate::tq::TqWarningKind| -> &str {
        match kind {
            crate::tq::TqWarningKind::NoAssertion => "TQ-001 No assertion",
            crate::tq::TqWarningKind::NoSut => "TQ-002 No SUT call",
            crate::tq::TqWarningKind::Untested => "TQ-003 Untested",
            crate::tq::TqWarningKind::Uncovered => "TQ-004 Uncovered",
            crate::tq::TqWarningKind::UntestedLogic { .. } => "TQ-005 Untested logic",
        }
    };
    let mut html = String::from(
        "<table>\n<thead><tr>\
         <th>Function</th><th>File</th><th>Line</th>\
         <th>Kind</th><th>Detail</th>\
         </tr></thead>\n<tbody>\n",
    );
    warnings.iter().for_each(|w| {
        let detail = match &w.kind {
            crate::tq::TqWarningKind::UntestedLogic { uncovered_lines } => {
                let lines: Vec<String> = uncovered_lines
                    .iter()
                    .map(|(kind, line)| format!("{} at line {line}", esc(kind)))
                    .collect();
                esc(&lines.join(", "))
            }
            _ => String::from("\u{2014}"),
        };
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td>\
             <td><span class=\"tag tag-warning\">{}</span></td>\
             <td>{}</td></tr>\n",
            esc(&w.function_name),
            esc(&w.file),
            w.line,
            kind_label(&w.kind),
            detail,
        ));
    });
    html.push_str("</tbody></table>\n");
    html
}
