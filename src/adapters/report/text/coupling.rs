use std::fmt::Write;

use colored::Colorize;

use super::views::{CouplingTableView, CouplingView, ModuleRow};
use crate::adapters::report::projections::coupling::{split_coupling_findings, SdpViolationRow};
use crate::domain::analysis_data::ModuleCouplingRecord;
use crate::domain::findings::CouplingFinding;

/// Project Coupling findings into the typed text View via the shared
/// `split_coupling_findings` helper.
pub(super) fn build_coupling_view(findings: &[CouplingFinding]) -> CouplingView {
    let buckets = split_coupling_findings(findings);
    CouplingView {
        cycle_paths: buckets.cycle_paths,
        sdp_violations: buckets.sdp_violations,
        structural_rows: buckets.structural_rows,
    }
}

/// Project Coupling module records into the typed table View.
pub(super) fn build_coupling_table_view(modules: &[ModuleCouplingRecord]) -> CouplingTableView {
    let modules = modules
        .iter()
        .map(|m| ModuleRow {
            name: m.module_name.clone(),
            afferent: m.afferent,
            efferent: m.efferent,
            instability: m.instability,
            suppressed: m.suppressed,
            warning: m.warning,
            incoming: m.incoming.clone(),
            outgoing: m.outgoing.clone(),
        })
        .collect();
    CouplingTableView { modules }
}

/// Format the coupling analysis section.
/// Always rendered (compact + verbose); verbose adds the legend +
/// per-module incoming/outgoing detail.
pub(super) fn format_coupling_section(
    findings: &CouplingView,
    table: &CouplingTableView,
    verbose: bool,
) -> String {
    let mut out = String::new();
    push_header(&mut out, table, verbose);
    push_cycles(&mut out, &findings.cycle_paths);
    push_sdp_violations(&mut out, &findings.sdp_violations);
    push_table(&mut out, table, verbose);
    push_cycle_status(&mut out, &findings.cycle_paths);
    out
}

fn push_header(out: &mut String, table: &CouplingTableView, verbose: bool) {
    let _ = writeln!(out, "\n{}", "═══ Coupling ═══".bold());
    if verbose {
        let _ = writeln!(out, "  Modules analyzed: {}", table.modules.len());
    }
}

fn push_cycles(out: &mut String, cycle_paths: &[Vec<String>]) {
    cycle_paths.iter().for_each(|path| {
        let _ = writeln!(
            out,
            "  {} Circular dependency: {}",
            "✗".red(),
            path.join(" → "),
        );
    });
}

fn push_sdp_violations(out: &mut String, rows: &[SdpViolationRow]) {
    rows.iter().for_each(|r| {
        let _ = writeln!(
            out,
            "  {} SDP violation: {} (I={:.2}) depends on {} (I={:.2})",
            "⚠".yellow(),
            r.from,
            r.from_instability,
            r.to,
            r.to_instability,
        );
    });
}

fn push_table(out: &mut String, table: &CouplingTableView, verbose: bool) {
    if verbose {
        push_legend(out, &table.modules);
    } else if !table.modules.is_empty() {
        let _ = writeln!(
            out,
            "\n    {:<20} {:>3}  {:>3}  Instability",
            "", "In", "Out"
        );
    }
    push_rows(out, &table.modules, verbose);
}

fn push_legend(out: &mut String, modules: &[ModuleRow]) {
    if !modules.is_empty() {
        let _ = writeln!(
            out,
            "\n  {} {}\n  {} {}\n  {} {}",
            "Incoming".dimmed(),
            "= modules depending on this one".dimmed(),
            "Outgoing".dimmed(),
            "= modules this one depends on".dimmed(),
            "Instability".dimmed(),
            "= Outgoing / (Incoming + Outgoing)".dimmed(),
        );
        let _ = writeln!(
            out,
            "\n    {:<20} {:>3}  {:>3}  Instability",
            "", "In", "Out",
        );
    }
}

fn push_rows(out: &mut String, modules: &[ModuleRow], verbose: bool) {
    let module_tag = |m: &ModuleRow| -> String {
        if m.suppressed {
            format!("  {}", "~ suppressed".yellow())
        } else if m.warning {
            format!("  {} exceeds threshold", "⚠".yellow())
        } else {
            String::new()
        }
    };

    modules.iter().for_each(|m| {
        let _ = writeln!(
            out,
            "    {:<20} {:>3}  {:>3}  {:.2}{}",
            m.name,
            m.afferent,
            m.efferent,
            m.instability,
            module_tag(m),
        );
        if verbose && !m.outgoing.is_empty() {
            let _ = writeln!(
                out,
                "      {} {}",
                "→ depends on:".dimmed(),
                m.outgoing.join(", "),
            );
        }
        if verbose && !m.incoming.is_empty() {
            let _ = writeln!(
                out,
                "      {} {}",
                "← used by:".dimmed(),
                m.incoming.join(", "),
            );
        }
    });
}

fn push_cycle_status(out: &mut String, cycle_paths: &[Vec<String>]) {
    if cycle_paths.is_empty() {
        let _ = writeln!(out, "\n  {} No circular dependencies.", "✓".green());
    }
}
