use colored::Colorize;

use crate::adapters::analyzers::iosp::{Classification, FunctionAnalysis};

/// Print refactoring suggestions for violation functions.
/// Integration: filters violations and delegates per-function suggestions.
pub fn print_suggestions(results: &[FunctionAnalysis]) {
    let violations: Vec<_> = results
        .iter()
        .filter(|f| !f.suppressed && matches!(f.classification, Classification::Violation { .. }))
        .collect();

    if violations.is_empty() {
        return;
    }

    println!("\n{}", "═══ Refactoring Suggestions ═══".bold());
    violations
        .iter()
        .for_each(|func| print_function_suggestion(func));
}

/// Logic kind constants for pattern matching without boolean chains.
const CONDITIONAL_KINDS: &[&str] = &["if", "match"];
const LOOP_KINDS: &[&str] = &["for", "while", "loop"];

/// Print a refactoring suggestion for a single violation function.
/// Operation: pattern matching logic, no own function calls.
fn print_function_suggestion(func: &FunctionAnalysis) {
    let Classification::Violation {
        logic_locations,
        call_locations,
        ..
    } = &func.classification
    else {
        return;
    };

    println!("\n  {} (line {})", func.qualified_name.bold(), func.line);

    let has_conditional = logic_locations
        .iter()
        .any(|l| CONDITIONAL_KINDS.contains(&l.kind.as_str()));
    let has_loop = logic_locations
        .iter()
        .any(|l| LOOP_KINDS.contains(&l.kind.as_str()));

    if has_conditional && !call_locations.is_empty() {
        println!(
            "    {} Extract the condition logic into a separate operation,",
            "→".cyan()
        );
        println!("      then call it from a pure integration function.");
    }

    if has_loop && !call_locations.is_empty() {
        println!(
            "    {} Consider using an iterator chain instead of a loop with calls,",
            "→".cyan()
        );
        println!("      or extract the loop body into a separate operation.");
    }

    if !has_conditional && !has_loop {
        println!(
            "    {} Extract the logic (arithmetic/comparisons) into a helper operation,",
            "→".cyan()
        );
        println!("      keeping this function as a pure integration.");
    }
}
