use super::html_escape;

/// Build the SRP analysis section — Integration: delegates to header builder + generic table builder.
pub(super) fn html_srp_section(srp: Option<&crate::srp::SrpAnalysis>) -> String {
    let esc = |s: &str| html_escape(s);
    let mut html = html_srp_header(srp);
    html.push_str(&html_srp_table(
        "Struct Warnings",
        "<th>Struct</th><th>File</th><th>Line</th>\
         <th>LCOM4</th><th>Fields</th><th>Methods</th><th>Fan-out</th><th>Score</th>",
        srp.map(|s| s.struct_warnings.as_slice()).unwrap_or(&[]),
        |w: &crate::srp::SrpWarning| w.suppressed,
        |w: &crate::srp::SrpWarning| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td>\
                 <td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.2}</td></tr>\n",
                esc(&w.struct_name),
                esc(&w.file),
                w.line,
                w.lcom4,
                w.field_count,
                w.method_count,
                w.fan_out,
                w.composite_score,
            )
        },
    ));
    html.push_str(&html_srp_table(
        "Module Warnings",
        "<th>Module</th><th>File</th><th>Production Lines</th><th>Length Score</th><th>Clusters</th>",
        srp.map(|s| s.module_warnings.as_slice()).unwrap_or(&[]),
        |w: &crate::srp::ModuleSrpWarning| w.suppressed,
        |w: &crate::srp::ModuleSrpWarning| {
            let cluster_info = if w.independent_clusters > 0 {
                format!("{} clusters", w.independent_clusters)
            } else { String::from("\u{2014}") };
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{:.2}</td><td>{}</td></tr>\n",
                esc(&w.module), esc(&w.file), w.production_lines, w.length_score, cluster_info,
            )
        },
    ));
    html.push_str(&html_srp_table(
        "Too-Many-Arguments Warnings",
        "<th>Function</th><th>File</th><th>Line</th><th>Params</th>",
        srp.map(|s| s.param_warnings.as_slice()).unwrap_or(&[]),
        |w: &crate::srp::ParamSrpWarning| w.suppressed,
        |w: &crate::srp::ParamSrpWarning| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                esc(&w.function_name),
                esc(&w.file),
                w.line,
                w.parameter_count,
            )
        },
    ));
    html.push_str("</div>\n</details>\n\n");
    html
}

/// Build the SRP section header with summary and details wrapper.
/// Operation: formatting logic, no own calls.
fn html_srp_header(srp: Option<&crate::srp::SrpAnalysis>) -> String {
    let (struct_count, module_count, param_count) = srp
        .map(|s| {
            (
                s.struct_warnings.iter().filter(|w| !w.suppressed).count(),
                s.module_warnings.iter().filter(|w| !w.suppressed).count(),
                s.param_warnings.iter().filter(|w| !w.suppressed).count(),
            )
        })
        .unwrap_or((0, 0, 0));
    let total = struct_count + module_count + param_count;

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
    html
}

/// Build a generic SRP warning table with the given title, headers, items, and row formatter.
/// Operation: formatting logic with closures, no own calls.
fn html_srp_table<T>(
    title: &str,
    headers: &str,
    items: &[T],
    is_suppressed: impl Fn(&T) -> bool,
    format_row: impl Fn(&T) -> String,
) -> String {
    let active: Vec<_> = items.iter().filter(|w| !is_suppressed(w)).collect();
    if active.is_empty() {
        return String::new();
    }
    let mut html = format!(
        "<h3>{title}</h3>\n<table>\n<thead><tr>\
         {headers}\
         </tr></thead>\n<tbody>\n"
    );
    active.iter().for_each(|w| html.push_str(&format_row(w)));
    html.push_str("</tbody></table>\n");
    html
}
