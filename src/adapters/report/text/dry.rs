//! Text DRY section: duplicates, fragments, dead code, boilerplate,
//! wildcards, repeated matches.

use std::fmt::Write;

use colored::Colorize;

use super::views::DryView;
use crate::adapters::report::projections::dry::{
    split_dry_findings, BoilerplateRow, DeadCodeRow, DryGroupRow, ParticipantRow, WildcardRow,
};
use crate::domain::findings::DryFinding;

/// Project DRY findings into the typed text View via the shared
/// `split_dry_findings` helper.
pub(super) fn build_dry_view(findings: &[DryFinding]) -> DryView {
    let buckets = split_dry_findings(findings);
    DryView {
        duplicate_groups: buckets.duplicate_groups,
        fragment_groups: buckets.fragment_groups,
        repeated_match_groups: buckets.repeated_match_groups,
        dead_code: buckets.dead_code,
        boilerplate: buckets.boilerplate,
        wildcards: buckets.wildcards,
    }
}

/// Format the DRY section from the View.
pub(super) fn format_dry_section(view: &DryView) -> String {
    if view.duplicate_groups.is_empty()
        && view.fragment_groups.is_empty()
        && view.dead_code.is_empty()
        && view.boilerplate.is_empty()
        && view.wildcards.is_empty()
        && view.repeated_match_groups.is_empty()
    {
        return String::new();
    }
    let mut out = String::new();
    let _ = writeln!(out, "\n{}", "═══ DRY / Dead Code ═══".bold());
    push_duplicate_entries(&mut out, &view.duplicate_groups);
    push_fragment_entries(&mut out, &view.fragment_groups);
    push_dead_code_entries(&mut out, &view.dead_code);
    push_boilerplate_entries(&mut out, &view.boilerplate);
    push_wildcard_entries(&mut out, &view.wildcards);
    push_repeated_match_entries(&mut out, &view.repeated_match_groups);
    out
}

fn push_participant_row(out: &mut String, p: &ParticipantRow) {
    let _ = writeln!(out, "    - {} ({}:{})", p.function_name, p.file, p.line);
}

fn push_duplicate_entries(out: &mut String, groups: &[DryGroupRow]) {
    groups.iter().enumerate().for_each(|(i, g)| {
        let kind_label = if g.kind_label == "Exact" {
            "Exact duplicate"
        } else {
            "Near-duplicate"
        };
        let _ = writeln!(
            out,
            "  {} Group {}: {} ({} functions)",
            "⚠".yellow(),
            i + 1,
            kind_label,
            g.participants.len(),
        );
        g.participants
            .iter()
            .for_each(|p| push_participant_row(out, p));
    });
}

fn push_fragment_entries(out: &mut String, groups: &[DryGroupRow]) {
    groups.iter().enumerate().for_each(|(i, g)| {
        let _ = writeln!(
            out,
            "  {} Fragment {}: {} matching statements",
            "⚠".yellow(),
            i + 1,
            g.kind_label,
        );
        g.participants
            .iter()
            .for_each(|p| push_participant_row(out, p));
    });
}

fn push_dead_code_entries(out: &mut String, rows: &[DeadCodeRow]) {
    rows.iter().for_each(|r| {
        let _ = writeln!(
            out,
            "  {} {} [{}] ({}:{}) — {}",
            "⚠".yellow(),
            r.qualified_name,
            r.kind_tag,
            r.file,
            r.line,
            r.suggestion,
        );
    });
}

fn push_boilerplate_entries(out: &mut String, rows: &[BoilerplateRow]) {
    rows.iter().for_each(|r| {
        let name = if r.struct_name.is_empty() {
            "(anonymous)"
        } else {
            r.struct_name.as_str()
        };
        let _ = writeln!(
            out,
            "  {} [{}] {} ({}:{}) — {}",
            "⚠".yellow(),
            r.pattern_id,
            name,
            r.file,
            r.line,
            r.message,
        );
        let _ = writeln!(out, "    → {}", r.suggestion.dimmed());
    });
}

fn push_wildcard_entries(out: &mut String, rows: &[WildcardRow]) {
    rows.iter().for_each(|r| {
        let _ = writeln!(
            out,
            "  {} Wildcard import: {} ({}:{})",
            "⚠".yellow(),
            r.module_path,
            r.file,
            r.line,
        );
    });
}

fn push_repeated_match_entries(out: &mut String, groups: &[DryGroupRow]) {
    groups.iter().enumerate().for_each(|(i, g)| {
        let _ = writeln!(
            out,
            "  {} Repeated match [{}] Group {}: {} instances",
            "⚠".yellow(),
            g.kind_label,
            i + 1,
            g.participants.len(),
        );
        g.participants
            .iter()
            .for_each(|p| push_participant_row(out, p));
    });
}
