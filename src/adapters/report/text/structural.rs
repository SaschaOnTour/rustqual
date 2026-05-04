//! Cross-dimension Structural-Checks section.
//!
//! Reads `structural_rows` from both `SrpView` and `CouplingView` —
//! each side projects its `Structural` variant into the same
//! `StructuralRow` shape, this formatter merges them into one section.

use std::fmt::Write;

use colored::Colorize;

use super::views::StructuralRow;

/// Format the Structural-Checks section from the SRP and Coupling
/// structural-row collections. Empty if both are empty.
pub(super) fn format_structural_section(
    srp_rows: &[StructuralRow],
    coupling_rows: &[StructuralRow],
) -> String {
    if srp_rows.is_empty() && coupling_rows.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let _ = writeln!(out);
    let _ = writeln!(out, "{}", "═══ Structural Checks ═══".bold());

    srp_rows.iter().chain(coupling_rows.iter()).for_each(|r| {
        let _ = writeln!(
            out,
            "  {} {}  {} ({}:{}) — {}",
            "\u{26a0}".yellow(),
            r.code,
            r.name,
            r.file,
            r.line,
            r.detail,
        );
    });
    out
}
