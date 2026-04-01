pub(super) use super::tq_table::html_tq_section;

use super::html_escape;
use crate::analyzer::PERCENTAGE_MULTIPLIER;

/// Build the DRY findings section: duplicates, fragments, dead code, boilerplate, wildcards, repeated matches.
/// Integration: assembles per-category HTML builders.
pub(super) fn html_dry_section(analysis: &crate::report::AnalysisResult) -> String {
    let mut html = html_dry_header(analysis);
    html.push_str(&html_duplicates_category(&analysis.duplicates));
    html.push_str(&html_fragments_category(&analysis.fragments));
    html.push_str(&html_dead_code_table(&analysis.dead_code));
    html.push_str(&html_boilerplate_table(&analysis.boilerplate));
    html.push_str(&html_wildcard_table(&analysis.wildcard_warnings));
    html.push_str(&html_repeated_matches_table(&analysis.repeated_matches));
    html.push_str("</div>\n</details>\n\n");
    html
}

/// Build DRY section header with finding count and empty state.
/// Operation: formatting logic, no own calls.
fn html_dry_header(analysis: &crate::report::AnalysisResult) -> String {
    let wildcards = analysis.wildcard_warnings.iter().filter(|w| !w.suppressed).count();
    let total = analysis.duplicates.len()
        + analysis.fragments.len()
        + analysis.dead_code.len()
        + analysis.boilerplate.len()
        + wildcards
        + analysis.repeated_matches.len();
    let mut html = String::new();
    html.push_str(&format!(
        "<details>\n<summary>DRY \u{2014} {} Finding{}</summary>\n\
         <div class=\"detail-content\">\n",
        total,
        if total == 1 { "" } else { "s" },
    ));
    if total == 0 {
        html.push_str("<p class=\"empty-state\">No DRY issues found.</p>\n");
    }
    html
}

/// Build HTML for duplicate function groups.
/// Operation: iteration and formatting logic, no own calls (html_escape via closure).
fn html_duplicates_category(duplicates: &[crate::dry::functions::DuplicateGroup]) -> String {
    if duplicates.is_empty() {
        return String::new();
    }
    let esc = |s: &str| html_escape(s);
    let mut html = String::from("<h3>Duplicate Functions</h3>\n");
    duplicates.iter().enumerate().for_each(|(i, g)| {
        let kind_label = match &g.kind {
            crate::dry::functions::DuplicateKind::Exact => "Exact".to_string(),
            crate::dry::functions::DuplicateKind::NearDuplicate { similarity } => {
                format!("{:.0}% similar", similarity * PERCENTAGE_MULTIPLIER)
            }
        };
        html.push_str(&format!(
            "<p><strong>Group {}</strong>: {} ({} functions)</p>\n<ul>\n",
            i + 1,
            esc(&kind_label),
            g.entries.len(),
        ));
        g.entries.iter().for_each(|e| {
            html.push_str(&format!(
                "  <li>{} ({}:{})</li>\n",
                esc(&e.qualified_name),
                esc(&e.file),
                e.line,
            ));
        });
        html.push_str("</ul>\n");
    });
    html
}

/// Build HTML for duplicate fragment groups.
/// Operation: iteration and formatting logic, no own calls (html_escape via closure).
fn html_fragments_category(fragments: &[crate::dry::fragments::FragmentGroup]) -> String {
    if fragments.is_empty() {
        return String::new();
    }
    let esc = |s: &str| html_escape(s);
    let mut html = String::from("<h3>Duplicate Fragments</h3>\n");
    fragments.iter().enumerate().for_each(|(i, g)| {
        html.push_str(&format!(
            "<p><strong>Fragment {}</strong>: {} matching statements</p>\n<ul>\n",
            i + 1,
            g.statement_count,
        ));
        g.entries.iter().for_each(|e| {
            html.push_str(&format!(
                "  <li>{} ({}:{}\u{2013}{})</li>\n",
                esc(&e.qualified_name),
                esc(&e.file),
                e.start_line,
                e.end_line,
            ));
        });
        html.push_str("</ul>\n");
    });
    html
}

/// Build HTML table for dead code warnings.
/// Operation: iteration and formatting logic, no own calls (html_escape via closure).
fn html_dead_code_table(dead_code: &[crate::dry::dead_code::DeadCodeWarning]) -> String {
    if dead_code.is_empty() {
        return String::new();
    }
    let esc = |s: &str| html_escape(s);
    let mut html = String::from(
        "<h3>Dead Code</h3>\n<table>\n<thead><tr>\
         <th>Function</th><th>File</th><th>Line</th>\
         <th>Kind</th><th>Suggestion</th>\
         </tr></thead>\n<tbody>\n",
    );
    dead_code.iter().for_each(|w| {
        let kind_tag = match w.kind {
            crate::dry::dead_code::DeadCodeKind::Uncalled => "uncalled",
            crate::dry::dead_code::DeadCodeKind::TestOnly => "test-only",
        };
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td>\
             <td><span class=\"tag tag-warning\">{kind_tag}</span></td>\
             <td>{}</td></tr>\n",
            esc(&w.qualified_name),
            esc(&w.file),
            w.line,
            esc(&w.suggestion),
        ));
    });
    html.push_str("</tbody></table>\n");
    html
}

/// Build HTML table for boilerplate pattern findings.
/// Operation: iteration and formatting logic, no own calls (html_escape via closure).
fn html_boilerplate_table(boilerplate: &[crate::dry::boilerplate::BoilerplateFind]) -> String {
    if boilerplate.is_empty() {
        return String::new();
    }
    let esc = |s: &str| html_escape(s);
    let mut html = String::from(
        "<h3>Boilerplate Patterns</h3>\n<table>\n<thead><tr>\
         <th>Pattern</th><th>Type</th><th>File</th><th>Line</th>\
         <th>Description</th><th>Suggestion</th>\
         </tr></thead>\n<tbody>\n",
    );
    boilerplate.iter().for_each(|b| {
        let name = b.struct_name.as_deref().unwrap_or("\u{2014}");
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td>\
             <td>{}</td><td>{}</td></tr>\n",
            esc(&b.pattern_id),
            esc(name),
            esc(&b.file),
            b.line,
            esc(&b.description),
            esc(&b.suggestion),
        ));
    });
    html.push_str("</tbody></table>\n");
    html
}

/// Build HTML table for wildcard import warnings.
/// Operation: iteration and formatting logic, no own calls (html_escape via closure).
fn html_wildcard_table(
    wildcard_warnings: &[crate::dry::wildcards::WildcardImportWarning],
) -> String {
    let active: Vec<_> = wildcard_warnings.iter().filter(|w| !w.suppressed).collect();
    if active.is_empty() {
        return String::new();
    }
    let esc = |s: &str| html_escape(s);
    let mut html = String::from(
        "<h3>Wildcard Imports</h3>\n<table>\n<thead><tr>\
         <th>Module Path</th><th>File</th><th>Line</th>\
         </tr></thead>\n<tbody>\n",
    );
    active.iter().for_each(|w| {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            esc(&w.module_path),
            esc(&w.file),
            w.line,
        ));
    });
    html.push_str("</tbody></table>\n");
    html
}

/// Build HTML table for repeated match pattern findings.
/// Operation: iteration and formatting logic, no own calls (html_escape via closure).
fn html_repeated_matches_table(
    repeated_matches: &[crate::dry::match_patterns::RepeatedMatchGroup],
) -> String {
    if repeated_matches.is_empty() {
        return String::new();
    }
    let esc = |s: &str| html_escape(s);
    let mut html = String::from(
        "<h3>Repeated Match Patterns</h3>\n<table>\n<thead><tr>\
         <th>Enum</th><th>Function</th><th>File</th><th>Line</th><th>Arms</th>\
         </tr></thead>\n<tbody>\n",
    );
    repeated_matches.iter().for_each(|g| {
        g.entries.iter().for_each(|e| {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                esc(&g.enum_name),
                esc(&e.function_name),
                esc(&e.file),
                e.line,
                e.arm_count,
            ));
        });
    });
    html.push_str("</tbody></table>\n");
    html
}

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
                esc(&w.struct_name), esc(&w.file), w.line,
                w.lcom4, w.field_count, w.method_count, w.fan_out, w.composite_score,
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
                esc(&w.function_name), esc(&w.file), w.line, w.parameter_count,
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

