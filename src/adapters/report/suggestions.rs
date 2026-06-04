use colored::Colorize;

use crate::adapters::analyzers::iosp::{Classification, FunctionAnalysis};

/// Logic kind constants for pattern matching without boolean chains.
const CONDITIONAL_KINDS: &[&str] = &["if", "match"];
const LOOP_KINDS: &[&str] = &["for", "while", "loop"];

/// A refactoring suggestion for one violation function: its name + line plus
/// the (headline, follow-up) message pairs that apply.
pub(crate) struct FunctionSuggestion {
    pub qualified_name: String,
    pub line: usize,
    pub messages: Vec<(&'static str, &'static str)>,
}

/// Build the refactoring suggestions for every unsuppressed violation. Pure so
/// the violation filter and the per-function branch logic are observable in
/// tests rather than buried in stdout.
/// Operation: filter + per-function projection (own call hidden in closure).
pub(crate) fn collect_suggestions(results: &[FunctionAnalysis]) -> Vec<FunctionSuggestion> {
    let lines = |f: &FunctionAnalysis| suggestion_messages(f);
    results
        .iter()
        .filter(|f| !f.suppressed && matches!(f.classification, Classification::Violation { .. }))
        .map(|f| FunctionSuggestion {
            qualified_name: f.qualified_name.clone(),
            line: f.line,
            messages: lines(f),
        })
        .collect()
}

/// The (headline, follow-up) suggestion messages that apply to a violation:
/// conditional+calls → extract condition, loop+calls → iterator chain, and
/// pure logic with neither → extract the logic. Pure.
/// Operation: pattern-matching logic, no own calls.
pub(crate) fn suggestion_messages(func: &FunctionAnalysis) -> Vec<(&'static str, &'static str)> {
    let Classification::Violation {
        logic_locations,
        call_locations,
        ..
    } = &func.classification
    else {
        return vec![];
    };
    let has_conditional = logic_locations
        .iter()
        .any(|l| CONDITIONAL_KINDS.contains(&l.kind.as_str()));
    let has_loop = logic_locations
        .iter()
        .any(|l| LOOP_KINDS.contains(&l.kind.as_str()));
    let has_calls = !call_locations.is_empty();
    let mut out = Vec::new();
    if has_conditional && has_calls {
        out.push((
            "Extract the condition logic into a separate operation,",
            "then call it from a pure integration function.",
        ));
    }
    if has_loop && has_calls {
        out.push((
            "Consider using an iterator chain instead of a loop with calls,",
            "or extract the loop body into a separate operation.",
        ));
    }
    if !has_conditional && !has_loop {
        out.push((
            "Extract the logic (arithmetic/comparisons) into a helper operation,",
            "keeping this function as a pure integration.",
        ));
    }
    out
}

/// Print refactoring suggestions for violation functions.
/// Integration: collects suggestions, then prints each.
pub fn print_suggestions(results: &[FunctionAnalysis]) {
    let suggestions = collect_suggestions(results);
    suggestions
        .first()
        .into_iter()
        .for_each(|_| println!("\n{}", "═══ Refactoring Suggestions ═══".bold()));
    suggestions.iter().for_each(print_function_suggestion);
}

/// Print one function's suggestion block.
/// Integration: prints header + each message line via closures.
fn print_function_suggestion(s: &FunctionSuggestion) {
    println!("\n  {} (line {})", s.qualified_name.bold(), s.line);
    s.messages.iter().for_each(|(headline, follow_up)| {
        println!("    {} {headline}", "→".cyan());
        println!("      {follow_up}");
    });
}
