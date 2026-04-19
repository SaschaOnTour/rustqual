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
