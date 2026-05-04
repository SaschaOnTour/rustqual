//! Shared Test-Quality projection: turn `&[TqFinding]` into typed
//! display rows.

use crate::domain::findings::{TqFinding, TqFindingKind};

/// Atomic TQ row.
pub(crate) struct TqRow {
    pub function_name: String,
    pub file: String,
    pub line: usize,
    pub display_label: &'static str,
    /// Pre-joined uncovered-line list ("if at line 5, for at line 9")
    /// or empty when no detail applies.
    pub detail: String,
}

/// Project TQ findings into display rows. Filters suppressed.
pub(crate) fn project_tq_rows(findings: &[TqFinding]) -> Vec<TqRow> {
    findings
        .iter()
        .filter(|f| !f.common.suppressed)
        .map(|f| TqRow {
            function_name: f.function_name.clone(),
            file: f.common.file.clone(),
            line: f.common.line,
            display_label: f.kind.meta().display_label,
            detail: tq_detail_text(&f.kind, &f.uncovered_lines),
        })
        .collect()
}

fn tq_detail_text(kind: &TqFindingKind, uncovered: &Option<Vec<(String, usize)>>) -> String {
    match (kind, uncovered) {
        (TqFindingKind::UntestedLogic, Some(lines)) => lines
            .iter()
            .map(|(k, line)| format!("{k} at line {line}"))
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    }
}
