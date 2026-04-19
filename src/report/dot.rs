use crate::adapters::analyzers::iosp::{Classification, FunctionAnalysis};

/// Print results as Graphviz DOT format for call-graph visualization.
/// Operation: string formatting logic with no own function calls.
pub fn print_dot(results: &[FunctionAnalysis]) {
    // Closure to sanitize names for DOT node IDs (eliminates 2× duplication)
    let sanitize = |name: &str| -> String {
        name.replace("::", "_")
            .replace(['.', '<', '>', ','], "_")
            .replace(['(', ')'], "")
    };

    println!("digraph rustqual {{");
    println!("  rankdir=LR;");
    println!("  node [shape=box, style=filled, fontname=\"Helvetica\"];");

    for func in results {
        if func.suppressed {
            continue;
        }

        let node_id = format!("\"{}\"", sanitize(&func.qualified_name));

        let (color, fontcolor) = match &func.classification {
            Classification::Integration => ("#c8e6c9", "#1b5e20"), // green
            Classification::Operation => ("#bbdefb", "#0d47a1"),   // blue
            Classification::Trivial => ("#f5f5f5", "#9e9e9e"),     // grey
            Classification::Violation { .. } => ("#ffcdd2", "#b71c1c"), // red
        };

        println!(
            "  {node_id} [label=\"{}\", fillcolor=\"{color}\", fontcolor=\"{fontcolor}\"];",
            func.qualified_name,
        );

        // Draw edges for own calls (only available for violations)
        if let Classification::Violation { call_locations, .. } = &func.classification {
            for call in call_locations {
                let target = format!("\"{}\"", sanitize(&call.name));
                println!("  {node_id} -> {target};");
            }
        }
    }

    println!("}}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::analyzers::iosp::{compute_severity, CallOccurrence, LogicOccurrence};

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
    fn test_print_dot_empty_no_panic() {
        let results: Vec<FunctionAnalysis> = vec![];
        print_dot(&results);
    }

    #[test]
    fn test_print_dot_integration_no_panic() {
        let results = vec![make_result("orchestrator", Classification::Integration)];
        print_dot(&results);
    }

    #[test]
    fn test_print_dot_violation_no_panic() {
        let results = vec![make_result(
            "bad_fn",
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
        print_dot(&results);
    }

    #[test]
    fn test_print_dot_suppressed_skipped() {
        let mut func = make_result("suppressed", Classification::Operation);
        func.suppressed = true;
        let results = vec![func];
        print_dot(&results);
    }

    #[test]
    fn test_print_dot_all_classifications() {
        let results = vec![
            make_result("integration_fn", Classification::Integration),
            make_result("operation_fn", Classification::Operation),
            make_result("trivial_fn", Classification::Trivial),
            make_result(
                "violation_fn",
                Classification::Violation {
                    has_logic: true,
                    has_own_calls: true,
                    logic_locations: vec![LogicOccurrence {
                        kind: "for".into(),
                        line: 1,
                    }],
                    call_locations: vec![CallOccurrence {
                        name: "foo".into(),
                        line: 2,
                    }],
                },
            ),
        ];
        print_dot(&results);
    }
}
