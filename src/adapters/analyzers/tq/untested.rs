use std::collections::{HashMap, HashSet, VecDeque};

use crate::adapters::analyzers::dry::dead_code::DeadCodeWarning;
use crate::adapters::analyzers::dry::DeclaredFunction;
use crate::config::Config;

use super::{TqWarning, TqWarningKind};

/// Build the transitive closure of tested functions from seed calls and a call graph.
/// Operation: iterative BFS over call graph, no own calls.
pub(crate) fn build_transitive_tested_set(
    seed: &HashSet<String>,
    call_graph: &HashMap<String, Vec<String>>,
) -> HashSet<String> {
    let mut tested = seed.clone();
    let mut queue: VecDeque<String> = seed.iter().cloned().collect();
    while let Some(name) = queue.pop_front() {
        if let Some(callees) = call_graph.get(&name) {
            callees.iter().for_each(|callee| {
                if tested.insert(callee.clone()) {
                    queue.push_back(callee.clone());
                }
            });
        }
    }
    tested
}

/// Detect production functions that are called from prod but never from any test (TQ-003).
/// Operation: set comparison logic, no own calls.
pub(crate) fn detect_untested_functions(
    declared_fns: &[DeclaredFunction],
    prod_calls: &HashSet<String>,
    transitive_tested: &HashSet<String>,
    dead_code: &[DeadCodeWarning],
    config: &Config,
) -> Vec<TqWarning> {
    // Dead code functions are already flagged — skip them for TQ-003
    let dead_names: HashSet<&str> = dead_code.iter().map(|d| d.function_name.as_str()).collect();

    declared_fns
        .iter()
        .filter(|f| {
            !f.is_test
                && !f.is_main
                && !f.has_allow_dead_code
                && !f.is_api
                && !f.is_test_helper
                && !f.is_trait_impl
                && !config.is_ignored_function(&f.name)
                && !dead_names.contains(f.name.as_str())
                && prod_calls.contains(&f.name)
                && !transitive_tested.contains(&f.name)
        })
        .map(|f| TqWarning {
            file: f.file.clone(),
            line: f.line,
            function_name: f.name.clone(),
            kind: TqWarningKind::Untested,
            suppressed: false,
        })
        .collect()
}
