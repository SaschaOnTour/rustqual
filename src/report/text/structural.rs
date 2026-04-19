use colored::Colorize;

use crate::adapters::analyzers::structural::StructuralAnalysis;

/// Print structural warnings grouped by rule code.
/// Operation: formatting logic with iteration, no own calls.
pub(crate) fn print_structural_section(structural: &StructuralAnalysis) {
    let warnings: Vec<_> = structural
        .warnings
        .iter()
        .filter(|w| !w.suppressed)
        .collect();
    if warnings.is_empty() {
        return;
    }
    println!();
    println!("{}", "═══ Structural Checks ═══".bold());

    warnings.iter().for_each(|w| {
        let (code, detail) = (w.kind.code(), w.kind.detail());
        println!(
            "  {} {code}  {} ({}:{}) — {}",
            "\u{26a0}".yellow(),
            w.name,
            w.file,
            w.line,
            detail,
        );
    });
}
