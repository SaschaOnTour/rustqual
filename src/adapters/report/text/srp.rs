use std::fmt::Write;

use colored::Colorize;

use super::views::SrpView;
use crate::adapters::report::projections::srp::{
    split_srp_findings, SrpModuleRow, SrpParamRow, SrpStructRow,
};
use crate::domain::findings::SrpFinding;

/// Project SRP findings into the typed text View. Splits via the
/// shared `split_srp_findings` helper; the bucket row types come from
/// `report::projections::srp` (shared with html).
pub(super) fn build_srp_view(findings: &[SrpFinding]) -> SrpView {
    let buckets = split_srp_findings(findings);
    SrpView {
        struct_warnings: buckets.struct_warnings,
        module_warnings: buckets.module_warnings,
        param_warnings: buckets.param_warnings,
        structural_rows: buckets.structural_rows,
    }
}

/// Format the SRP section. Excludes the structural rows — those go into
/// the cross-dimension Structural section.
pub(super) fn format_srp_section(view: &SrpView) -> String {
    if view.struct_warnings.is_empty()
        && view.module_warnings.is_empty()
        && view.param_warnings.is_empty()
    {
        return String::new();
    }
    let mut out = String::new();
    let _ = writeln!(out, "\n{}", "═══ SRP Analysis ═══".bold());
    push_struct_warnings(&mut out, &view.struct_warnings);
    push_module_warnings(&mut out, &view.module_warnings);
    push_param_warnings(&mut out, &view.param_warnings);
    out
}

fn push_struct_warnings(out: &mut String, rows: &[SrpStructRow]) {
    rows.iter().for_each(|r| {
        let _ = writeln!(
            out,
            "  {} {} ({}:{}) — LCOM4={}, fields={}, methods={}, fan-out={}",
            "⚠".yellow(),
            r.struct_name,
            r.file,
            r.line,
            r.lcom4,
            r.field_count,
            r.method_count,
            r.fan_out,
        );
    });
}

fn push_module_warnings(out: &mut String, rows: &[SrpModuleRow]) {
    rows.iter().for_each(|r| {
        if r.production_lines > 0 {
            let _ = writeln!(
                out,
                "  {} {} — {} production lines",
                "⚠".yellow(),
                r.module,
                r.production_lines,
            );
        }
        if r.independent_clusters > 0 {
            let _ = writeln!(
                out,
                "  {} {} — {} independent function clusters",
                "⚠".yellow(),
                r.module,
                r.independent_clusters,
            );
            r.cluster_names.iter().enumerate().for_each(|(i, names)| {
                let _ = writeln!(out, "    Cluster {}: [{}]", i + 1, names);
            });
        }
    });
}

fn push_param_warnings(out: &mut String, rows: &[SrpParamRow]) {
    rows.iter().for_each(|r| {
        let _ = writeln!(
            out,
            "  {} {} ({}:{}) — {} parameters (exceeds threshold)",
            "⚠".yellow(),
            r.function_name,
            r.file,
            r.line,
            r.parameter_count,
        );
    });
}
