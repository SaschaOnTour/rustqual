pub(super) use super::srp_tables::html_srp_section;
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
    let wildcards = analysis
        .wildcard_warnings
        .iter()
        .filter(|w| !w.suppressed)
        .count();
    let total = analysis.duplicates.iter().filter(|g| !g.suppressed).count()
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
    if duplicates.iter().all(|g| g.suppressed) {
        return String::new();
    }
    let esc = |s: &str| html_escape(s);
    let mut html = String::from("<h3>Duplicate Functions</h3>\n");
    duplicates
        .iter()
        .filter(|g| !g.suppressed)
        .enumerate()
        .for_each(|(i, g)| {
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
    fragments
        .iter()
        .filter(|g| !g.suppressed)
        .enumerate()
        .for_each(|(i, g)| {
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
    boilerplate.iter().filter(|b| !b.suppressed).for_each(|b| {
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
    repeated_matches
        .iter()
        .filter(|g| !g.suppressed)
        .for_each(|g| {
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

// SRP section is in super::srp_tables.
