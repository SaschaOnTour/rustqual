use colored::Colorize;

use crate::analyzer::{Classification, FunctionAnalysis};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::{compute_severity, CallOccurrence, LogicOccurrence};

    fn make_result(name: &str, classification: Classification) -> FunctionAnalysis {
        let severity = compute_severity(&classification);
        FunctionAnalysis {
            name: name.to_string(),
            file: "test.rs".to_string(),
            line: 1,
            classification,
            parent_type: None,
            suppressed: false,
            complexity: None,
            qualified_name: name.to_string(),
            severity,
            cognitive_warning: false,
            cyclomatic_warning: false,
            nesting_depth_warning: false,
            function_length_warning: false,
            unsafe_warning: false,
            error_handling_warning: false,
            complexity_suppressed: false,
            own_calls: vec![],
            parameter_count: 0,
            is_trait_impl: false,
            is_test: false,
            effort_score: None,
        }
    }

    #[test]
    fn test_print_suggestions_no_violations() {
        let results = vec![
            make_result("a", Classification::Integration),
            make_result("b", Classification::Operation),
        ];
        print_suggestions(&results);
    }

    #[test]
    fn test_print_suggestions_with_if_logic() {
        let results = vec![make_result(
            "if_fn",
            Classification::Violation {
                has_logic: true,
                has_own_calls: true,
                logic_locations: vec![LogicOccurrence {
                    kind: "if".into(),
                    line: 1,
                }],
                call_locations: vec![CallOccurrence {
                    name: "helper".into(),
                    line: 2,
                }],
            },
        )];
        print_suggestions(&results);
    }

    #[test]
    fn test_print_suggestions_with_loop_logic() {
        let results = vec![make_result(
            "loop_fn",
            Classification::Violation {
                has_logic: true,
                has_own_calls: true,
                logic_locations: vec![LogicOccurrence {
                    kind: "for".into(),
                    line: 1,
                }],
                call_locations: vec![CallOccurrence {
                    name: "helper".into(),
                    line: 2,
                }],
            },
        )];
        print_suggestions(&results);
    }

    #[test]
    fn test_print_suggestions_with_arithmetic_logic() {
        let results = vec![make_result(
            "arith_fn",
            Classification::Violation {
                has_logic: true,
                has_own_calls: true,
                logic_locations: vec![LogicOccurrence {
                    kind: "arithmetic".into(),
                    line: 1,
                }],
                call_locations: vec![CallOccurrence {
                    name: "helper".into(),
                    line: 2,
                }],
            },
        )];
        print_suggestions(&results);
    }

    #[test]
    fn test_print_suggestions_suppressed_skipped() {
        let mut func = make_result(
            "suppressed_fn",
            Classification::Violation {
                has_logic: true,
                has_own_calls: true,
                logic_locations: vec![LogicOccurrence {
                    kind: "if".into(),
                    line: 1,
                }],
                call_locations: vec![CallOccurrence {
                    name: "f".into(),
                    line: 2,
                }],
            },
        );
        func.suppressed = true;
        let results = vec![func];
        print_suggestions(&results);
    }
}
