//! Flat-table renderers for the DRY section: dead code, boilerplate,
//! wildcards, repeated-matches.

use crate::adapters::report::html::html_escape;
use crate::adapters::report::html::views::{BoilerplateRow, DeadCodeRow, DryGroupRow, WildcardRow};

pub(super) fn format_dead_code_table(rows: &[DeadCodeRow]) -> String {
    let esc = |s: &str| html_escape(s);
    render_table(
        "Dead Code",
        &["Function", "File", "Line", "Kind", "Suggestion"],
        rows,
        |r| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td>\
                 <td><span class=\"tag tag-warning\">{}</span></td>\
                 <td>{}</td></tr>\n",
                esc(&r.qualified_name),
                esc(&r.file),
                r.line,
                r.kind_tag,
                esc(&r.suggestion),
            )
        },
    )
}

pub(super) fn format_boilerplate_table(rows: &[BoilerplateRow]) -> String {
    let esc = |s: &str| html_escape(s);
    render_table(
        "Boilerplate",
        &["Pattern", "Struct", "File", "Line", "Message", "Suggestion"],
        rows,
        |r| {
            let name = if r.struct_name.is_empty() {
                "(anonymous)"
            } else {
                r.struct_name.as_str()
            };
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td>\
                 <td>{}</td><td>{}</td></tr>\n",
                esc(&r.pattern_id),
                esc(name),
                esc(&r.file),
                r.line,
                esc(&r.message),
                esc(&r.suggestion),
            )
        },
    )
}

pub(super) fn format_wildcard_table(rows: &[WildcardRow]) -> String {
    let esc = |s: &str| html_escape(s);
    render_table("Wildcard Imports", &["Module", "File", "Line"], rows, |r| {
        format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            esc(&r.module_path),
            esc(&r.file),
            r.line,
        )
    })
}

pub(super) fn format_repeated_match_table(groups: &[DryGroupRow]) -> String {
    if groups.is_empty() {
        return String::new();
    }
    let esc = |s: &str| html_escape(s);
    let mut body = String::new();
    groups.iter().for_each(|g| {
        g.participants.iter().for_each(|p| {
            body.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                esc(&g.kind_label),
                esc(&p.function_name),
                esc(&p.file),
                p.line,
            ));
        });
    });
    let mut html = String::from(
        "<h3>Repeated Match Patterns</h3>\n<table>\n<thead><tr>\
         <th>Enum</th><th>Function</th><th>File</th><th>Line</th>\
         </tr></thead>\n<tbody>\n",
    );
    html.push_str(&body);
    html.push_str("</tbody></table>\n");
    html
}

fn render_table<T>(
    title: &str,
    headers: &[&str],
    rows: &[T],
    row_html: impl Fn(&T) -> String,
) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let mut html = format!("<h3>{title}</h3>\n<table>\n<thead><tr>");
    for h in headers {
        html.push_str(&format!("<th>{h}</th>"));
    }
    html.push_str("</tr></thead>\n<tbody>\n");
    rows.iter().for_each(|r| html.push_str(&row_html(r)));
    html.push_str("</tbody></table>\n");
    html
}
