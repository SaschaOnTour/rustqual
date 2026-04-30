//! Render `HtmlDryView` into HTML markup. Sub-renders for the
//! flat-table sections live in `tables`; group-style rendering and the
//! top-level orchestrator live here.

mod tables;

use crate::adapters::report::html::html_escape;
use crate::adapters::report::html::views::{DryGroupRow, HtmlDryView};

use tables::{
    format_boilerplate_table, format_dead_code_table, format_repeated_match_table,
    format_wildcard_table,
};

/// Render the DRY section HTML from the typed view.
pub(crate) fn format_dry_section(view: &HtmlDryView) -> String {
    let total = view.duplicate_groups.len()
        + view.fragment_groups.len()
        + view.dead_code.len()
        + view.boilerplate.len()
        + view.wildcards.len()
        + view.repeated_match_groups.len();
    let mut html = String::new();
    html.push_str(&format!(
        "<details>\n<summary>DRY \u{2014} {} Finding{}</summary>\n\
         <div class=\"detail-content\">\n",
        total,
        if total == 1 { "" } else { "s" },
    ));
    if total == 0 {
        html.push_str("<p class=\"empty-state\">No DRY issues found.</p>\n");
    } else {
        html.push_str(&format_group_section(
            "Duplicate Functions",
            &view.duplicate_groups,
            |i, kind, count| {
                format!(
                    "<strong>Group {}</strong>: {} ({} functions)",
                    i + 1,
                    html_escape(kind),
                    count,
                )
            },
        ));
        html.push_str(&format_group_section(
            "Duplicate Fragments",
            &view.fragment_groups,
            |i, label, _| {
                format!(
                    "<strong>Fragment {}</strong>: {} matching statements",
                    i + 1,
                    html_escape(label),
                )
            },
        ));
        html.push_str(&format_dead_code_table(&view.dead_code));
        html.push_str(&format_boilerplate_table(&view.boilerplate));
        html.push_str(&format_wildcard_table(&view.wildcards));
        html.push_str(&format_repeated_match_table(&view.repeated_match_groups));
    }
    html.push_str("</div>\n</details>\n\n");
    html
}

fn format_group_section(
    title: &str,
    groups: &[DryGroupRow],
    header_text: impl Fn(usize, &str, usize) -> String,
) -> String {
    if groups.is_empty() {
        return String::new();
    }
    let esc = |s: &str| html_escape(s);
    let mut html = format!("<h3>{title}</h3>\n");
    groups.iter().enumerate().for_each(|(i, g)| {
        html.push_str(&format!(
            "<p>{}</p>\n<ul>\n",
            header_text(i, &g.kind_label, g.participants.len())
        ));
        g.participants.iter().for_each(|p| {
            html.push_str(&format!(
                "  <li>{} ({}:{})</li>\n",
                esc(&p.function_name),
                esc(&p.file),
                p.line,
            ));
        });
        html.push_str("</ul>\n");
    });
    html
}
