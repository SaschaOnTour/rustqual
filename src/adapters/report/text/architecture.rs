//! Text rendering for the Architecture dimension.
//!
//! Two-step: `build_architecture_view` projects findings into a typed
//! `ArchitectureView`; `format_architecture_section` consumes the View
//! and produces the markup string.

use std::fmt::Write;

use colored::Colorize;

use super::views::{ArchitectureRow, ArchitectureView};
use crate::domain::findings::ArchitectureFinding;

/// Project finding slice into the typed text View.
pub(super) fn build_architecture_view(findings: &[ArchitectureFinding]) -> ArchitectureView {
    let rows: Vec<ArchitectureRow> = findings
        .iter()
        .filter(|f| !f.common.suppressed)
        .map(|f| ArchitectureRow {
            file: f.common.file.clone(),
            line: f.common.line,
            rule_id: f.common.rule_id.clone(),
            message: f.common.message.clone(),
        })
        .collect();
    ArchitectureView { findings: rows }
}

/// Format the Architecture findings section. Empty View → empty output
/// (no header noise).
pub(super) fn format_architecture_section(view: &ArchitectureView) -> String {
    if view.findings.is_empty() {
        return String::new();
    }
    let n = view.findings.len();
    let heading = format!(
        "\n═══ Architecture — {n} Finding{} ═══",
        if n == 1 { "" } else { "s" }
    );
    let mut out = String::new();
    let _ = writeln!(out, "{}", heading.bold());
    view.findings.iter().for_each(|r| {
        let _ = writeln!(
            out,
            "  {}:{}  {}  {}",
            r.file.dimmed(),
            r.line,
            r.rule_id.cyan(),
            r.message,
        );
    });
    out
}
