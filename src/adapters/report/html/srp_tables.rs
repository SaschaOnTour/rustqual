//! HTML SRP section: struct cohesion, module length, parameter count
//! sub-tables. The Structural variant is filtered out — it's rendered
//! by the cross-dim Structural section.

use super::html_escape;
use super::views::HtmlSrpView;
use crate::adapters::report::projections::srp::{
    split_srp_findings, SrpModuleRow, SrpParamRow, SrpStructRow,
};
use crate::domain::findings::SrpFinding;

/// Project SRP findings into the typed view via the shared splitter.
pub(super) fn build_srp_view(findings: &[SrpFinding]) -> HtmlSrpView {
    let buckets = split_srp_findings(findings);
    HtmlSrpView {
        struct_warnings: buckets.struct_warnings,
        module_warnings: buckets.module_warnings,
        param_warnings: buckets.param_warnings,
        structural_rows: buckets.structural_rows,
    }
}

pub(super) fn format_srp_section(view: &HtmlSrpView) -> String {
    let total = view.struct_warnings.len() + view.module_warnings.len() + view.param_warnings.len();
    let mut html = String::new();
    html.push_str(&format!(
        "<details>\n<summary>SRP \u{2014} {} Warning{}</summary>\n\
         <div class=\"detail-content\">\n",
        total,
        if total == 1 { "" } else { "s" },
    ));
    if total == 0 {
        html.push_str("<p class=\"empty-state\">No SRP warnings.</p>\n");
    }
    html.push_str(&format_struct_warnings(&view.struct_warnings));
    html.push_str(&format_module_warnings(&view.module_warnings));
    html.push_str(&format_param_warnings(&view.param_warnings));
    html.push_str("</div>\n</details>\n\n");
    html
}

fn html_table(title: &str, headers: &str, rows: Vec<String>) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let mut html =
        format!("<h3>{title}</h3>\n<table>\n<thead><tr>{headers}</tr></thead>\n<tbody>\n");
    rows.iter().for_each(|r| html.push_str(r));
    html.push_str("</tbody></table>\n");
    html
}

fn format_struct_warnings(rows: &[SrpStructRow]) -> String {
    let rendered: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td>\
                 <td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                html_escape(&r.struct_name),
                html_escape(&r.file),
                r.line,
                r.lcom4,
                r.field_count,
                r.method_count,
                r.fan_out,
            )
        })
        .collect();
    html_table(
        "Struct Warnings",
        "<th>Struct</th><th>File</th><th>Line</th>\
         <th>LCOM4</th><th>Fields</th><th>Methods</th><th>Fan-out</th>",
        rendered,
    )
}

fn format_module_warnings(rows: &[SrpModuleRow]) -> String {
    let rendered: Vec<String> = rows
        .iter()
        .map(|r| {
            let cluster_info = if r.independent_clusters > 0 {
                format!("{} clusters", r.independent_clusters)
            } else {
                String::from("\u{2014}")
            };
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                html_escape(&r.module),
                html_escape(&r.file),
                r.production_lines,
                cluster_info,
            )
        })
        .collect();
    html_table(
        "Module Warnings",
        "<th>Module</th><th>File</th><th>Production Lines</th><th>Clusters</th>",
        rendered,
    )
}

fn format_param_warnings(rows: &[SrpParamRow]) -> String {
    let rendered: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                html_escape(&r.function_name),
                html_escape(&r.file),
                r.line,
                r.parameter_count,
            )
        })
        .collect();
    html_table(
        "Too-Many-Arguments Warnings",
        "<th>Function</th><th>File</th><th>Line</th><th>Params</th>",
        rendered,
    )
}
