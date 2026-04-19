use colored::Colorize;

use crate::adapters::analyzers::tq::{TqAnalysis, TqWarningKind};

/// Print test quality warnings grouped by kind.
/// Operation: formatting logic with iteration, no own calls.
pub(crate) fn print_tq_section(tq: &TqAnalysis) {
    let warnings: Vec<_> = tq.warnings.iter().filter(|w| !w.suppressed).collect();
    if warnings.is_empty() {
        return;
    }
    println!();
    println!("{}", "═══ Test Quality ═══".bold());

    let kind_label = |kind: &TqWarningKind| -> &str {
        match kind {
            TqWarningKind::NoAssertion => "TQ-001  No assertion",
            TqWarningKind::NoSut => "TQ-002  No SUT call",
            TqWarningKind::Untested => "TQ-003  Untested",
            TqWarningKind::Uncovered => "TQ-004  Uncovered",
            TqWarningKind::UntestedLogic { .. } => "TQ-005  Untested logic",
        }
    };

    warnings.iter().for_each(|w| {
        let detail = match &w.kind {
            TqWarningKind::UntestedLogic { uncovered_lines } => {
                let lines: Vec<String> = uncovered_lines
                    .iter()
                    .map(|(kind, line)| format!("{kind} at line {line}"))
                    .collect();
                format!("    {}", lines.join(", "))
            }
            _ => String::new(),
        };
        println!(
            "  {} {} ({}:{}) — {}",
            "⚠".yellow(),
            w.function_name,
            w.file,
            w.line,
            kind_label(&w.kind),
        );
        if !detail.is_empty() {
            println!("{detail}");
        }
    });
}
