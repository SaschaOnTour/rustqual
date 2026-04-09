use colored::Colorize;

use crate::report::AnalysisResult;

/// Print DRY analysis section: duplicates, fragments, dead code, boilerplate, wildcards, repeated matches.
/// Integration: orchestrates per-category DRY printers.
pub fn print_dry_section(analysis: &AnalysisResult) {
    print_dry_header(analysis);
    print_duplicate_entries(&analysis.duplicates);
    print_fragment_entries(&analysis.fragments);
    print_dead_code_entries(&analysis.dead_code);
    print_boilerplate_entries(&analysis.boilerplate);
    print_wildcard_entries(&analysis.wildcard_warnings);
    print_repeated_match_entries(&analysis.repeated_matches);
}

/// Print DRY section header if there are any findings.
/// Operation: conditional formatting, no own calls.
fn print_dry_header(analysis: &AnalysisResult) {
    let has_wildcards = analysis.wildcard_warnings.iter().any(|w| !w.suppressed);
    if analysis.duplicates.is_empty()
        && analysis.dead_code.is_empty()
        && analysis.fragments.is_empty()
        && analysis.boilerplate.is_empty()
        && !has_wildcards
        && analysis.repeated_matches.is_empty()
    {
        return;
    }
    println!("\n{}", "═══ DRY / Dead Code ═══".bold());
}

/// Print repeated match pattern entries.
/// Operation: iteration and formatting logic, no own calls.
fn print_repeated_match_entries(
    repeated_matches: &[crate::dry::match_patterns::RepeatedMatchGroup],
) {
    for (i, group) in repeated_matches
        .iter()
        .filter(|g| !g.suppressed)
        .enumerate()
    {
        println!(
            "  {} Repeated match [{}] Group {}: {} arms, {} instances",
            "⚠".yellow(),
            group.enum_name,
            i + 1,
            group.entries.first().map(|e| e.arm_count).unwrap_or(0),
            group.entries.len(),
        );
        for entry in &group.entries {
            println!(
                "    - {} ({}:{})",
                entry.function_name, entry.file, entry.line,
            );
        }
    }
}

/// Print duplicate function group entries.
/// Operation: iteration and formatting logic, no own calls.
fn print_duplicate_entries(duplicates: &[crate::dry::functions::DuplicateGroup]) {
    for (i, group) in duplicates.iter().filter(|g| !g.suppressed).enumerate() {
        let kind_label = match &group.kind {
            crate::dry::functions::DuplicateKind::Exact => "Exact duplicate".to_string(),
            crate::dry::functions::DuplicateKind::NearDuplicate { similarity } => {
                format!("Near-duplicate ({:.0}% similar)", similarity * 100.0)
            }
        };
        println!(
            "  {} Group {}: {} ({} functions)",
            "⚠".yellow(),
            i + 1,
            kind_label,
            group.entries.len(),
        );
        for entry in &group.entries {
            println!(
                "    - {} ({}:{})",
                entry.qualified_name, entry.file, entry.line,
            );
        }
    }
}

/// Print duplicate fragment group entries.
/// Operation: iteration and formatting logic, no own calls.
fn print_fragment_entries(fragments: &[crate::dry::fragments::FragmentGroup]) {
    for (i, group) in fragments.iter().filter(|g| !g.suppressed).enumerate() {
        println!(
            "  {} Fragment {}: {} matching statements",
            "⚠".yellow(),
            i + 1,
            group.statement_count,
        );
        for entry in &group.entries {
            println!(
                "    - {} ({}:{}-{})",
                entry.qualified_name, entry.file, entry.start_line, entry.end_line,
            );
        }
    }
}

/// Print dead code warning entries.
/// Operation: iteration and formatting logic, no own calls.
fn print_dead_code_entries(dead_code: &[crate::dry::dead_code::DeadCodeWarning]) {
    for w in dead_code {
        let kind_tag = match w.kind {
            crate::dry::dead_code::DeadCodeKind::Uncalled => "uncalled",
            crate::dry::dead_code::DeadCodeKind::TestOnly => "test-only",
        };
        println!(
            "  {} {} [{}] ({}:{}) — {}",
            "⚠".yellow(),
            w.qualified_name,
            kind_tag,
            w.file,
            w.line,
            w.suggestion,
        );
    }
}

/// Print boilerplate pattern entries.
/// Operation: iteration and formatting logic, no own calls.
fn print_boilerplate_entries(boilerplate: &[crate::dry::boilerplate::BoilerplateFind]) {
    for bp in boilerplate.iter().filter(|b| !b.suppressed) {
        let name = bp.struct_name.as_deref().unwrap_or("(anonymous)");
        println!(
            "  {} [{}] {} ({}:{}) — {}",
            "⚠".yellow(),
            bp.pattern_id,
            name,
            bp.file,
            bp.line,
            bp.description,
        );
        println!("    → {}", bp.suggestion.dimmed());
    }
}

/// Print wildcard import warning entries.
/// Operation: iteration and formatting logic, no own calls.
fn print_wildcard_entries(wildcard_warnings: &[crate::dry::wildcards::WildcardImportWarning]) {
    for w in wildcard_warnings {
        if w.suppressed {
            continue;
        }
        println!(
            "  {} Wildcard import: {} ({}:{})",
            "⚠".yellow(),
            w.module_path,
            w.file,
            w.line,
        );
    }
}
