use std::fmt::Write;

use colored::Colorize;

use super::views::TqView;
use crate::adapters::report::projections::tq::{project_tq_rows, TqRow};
use crate::domain::findings::TqFinding;

/// Project TQ findings into the typed text View via the shared
/// `project_tq_rows` helper.
pub(super) fn build_tq_view(findings: &[TqFinding]) -> TqView {
    TqView {
        warnings: project_tq_rows(findings),
    }
}

/// Format the TQ section from the View.
pub(super) fn format_tq_section(view: &TqView) -> String {
    if view.warnings.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let _ = writeln!(out);
    let _ = writeln!(out, "{}", "═══ Test Quality ═══".bold());
    view.warnings.iter().for_each(|w| {
        push_tq_row(&mut out, w);
    });
    out
}

fn push_tq_row(out: &mut String, w: &TqRow) {
    let _ = writeln!(
        out,
        "  {} {} ({}:{}) — {}",
        "⚠".yellow(),
        w.function_name,
        w.file,
        w.line,
        w.display_label,
    );
    if !w.detail.is_empty() {
        let _ = writeln!(out, "    {}", w.detail);
    }
}
