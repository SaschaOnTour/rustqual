use std::collections::{HashMap, HashSet, VecDeque};

use crate::config::Config;
use crate::dry::dead_code::DeadCodeWarning;
use crate::dry::DeclaredFunction;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_declared(name: &str, is_test: bool) -> DeclaredFunction {
        DeclaredFunction {
            name: name.to_string(),
            qualified_name: name.to_string(),
            file: "lib.rs".to_string(),
            line: 1,
            is_test,
            is_main: false,
            is_trait_impl: false,
            has_allow_dead_code: false,
            is_api: false,
        }
    }

    #[test]
    fn test_untested_prod_fn_emits_warning() {
        let declared = vec![make_declared("process", false)];
        let prod_calls: HashSet<String> = ["process".to_string()].into();
        let tested = HashSet::new();
        let config = Config::default();

        let warnings =
            detect_untested_functions(&declared, &prod_calls, &tested, &[], &config);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].kind, TqWarningKind::Untested);
        assert_eq!(warnings[0].function_name, "process");
    }

    #[test]
    fn test_tested_fn_no_warning() {
        let declared = vec![make_declared("process", false)];
        let prod_calls: HashSet<String> = ["process".to_string()].into();
        let tested: HashSet<String> = ["process".to_string()].into();
        let config = Config::default();

        let warnings =
            detect_untested_functions(&declared, &prod_calls, &tested, &[], &config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_uncalled_fn_no_warning() {
        let declared = vec![make_declared("unused", false)];
        let prod_calls: HashSet<String> = HashSet::new();
        let tested = HashSet::new();
        let config = Config::default();

        let warnings =
            detect_untested_functions(&declared, &prod_calls, &tested, &[], &config);
        assert!(warnings.is_empty(), "functions not called from prod are not TQ-003");
    }

    #[test]
    fn test_test_fn_excluded() {
        let declared = vec![make_declared("test_helper", true)];
        let prod_calls: HashSet<String> = ["test_helper".to_string()].into();
        let tested = HashSet::new();
        let config = Config::default();

        let warnings =
            detect_untested_functions(&declared, &prod_calls, &tested, &[], &config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_main_fn_excluded() {
        let mut declared = vec![make_declared("main", false)];
        declared[0].is_main = true;
        let prod_calls: HashSet<String> = ["main".to_string()].into();
        let tested = HashSet::new();
        let config = Config::default();

        let warnings =
            detect_untested_functions(&declared, &prod_calls, &tested, &[], &config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_trait_impl_excluded() {
        let mut declared = vec![make_declared("fmt", false)];
        declared[0].is_trait_impl = true;
        let prod_calls: HashSet<String> = ["fmt".to_string()].into();
        let tested = HashSet::new();
        let config = Config::default();

        let warnings =
            detect_untested_functions(&declared, &prod_calls, &tested, &[], &config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_dead_code_excluded() {
        let declared = vec![make_declared("dead_fn", false)];
        let prod_calls: HashSet<String> = ["dead_fn".to_string()].into();
        let tested = HashSet::new();
        let dead = vec![crate::dry::dead_code::DeadCodeWarning {
            function_name: "dead_fn".to_string(),
            file: "lib.rs".to_string(),
            line: 1,
            kind: crate::dry::dead_code::DeadCodeKind::Uncalled,
            qualified_name: "dead_fn".to_string(),
            suggestion: String::new(),
        }];
        let config = Config::default();

        let warnings =
            detect_untested_functions(&declared, &prod_calls, &tested, &dead, &config);
        assert!(warnings.is_empty());
    }

    // ── Transitive closure tests ─────────────────────────────────────────

    #[test]
    fn test_transitive_tested_not_flagged() {
        // Test calls A, A calls B → B should not be flagged
        let declared = vec![make_declared("a", false), make_declared("b", false)];
        let prod_calls: HashSet<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        let test_calls: HashSet<String> = ["a".to_string()].into();
        let call_graph: HashMap<String, Vec<String>> =
            [("a".to_string(), vec!["b".to_string()])].into();
        let tested = build_transitive_tested_set(&test_calls, &call_graph);
        let config = Config::default();

        let warnings =
            detect_untested_functions(&declared, &prod_calls, &tested, &[], &config);
        assert!(warnings.is_empty(), "b is transitively tested via a");
    }

    #[test]
    fn test_deep_transitive_not_flagged() {
        // Test calls A, A→B→C → C should not be flagged
        let declared = vec![
            make_declared("a", false),
            make_declared("b", false),
            make_declared("c", false),
        ];
        let prod_calls: HashSet<String> =
            ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let test_calls: HashSet<String> = ["a".to_string()].into();
        let call_graph: HashMap<String, Vec<String>> = [
            ("a".to_string(), vec!["b".to_string()]),
            ("b".to_string(), vec!["c".to_string()]),
        ]
        .into();
        let tested = build_transitive_tested_set(&test_calls, &call_graph);
        let config = Config::default();

        let warnings =
            detect_untested_functions(&declared, &prod_calls, &tested, &[], &config);
        assert!(warnings.is_empty(), "c is transitively tested via a→b→c");
    }

    #[test]
    fn test_circular_calls_no_infinite_loop() {
        // A→B→A (cycle), test calls A → terminates without infinite loop
        let declared = vec![make_declared("a", false), make_declared("b", false)];
        let prod_calls: HashSet<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        let test_calls: HashSet<String> = ["a".to_string()].into();
        let call_graph: HashMap<String, Vec<String>> = [
            ("a".to_string(), vec!["b".to_string()]),
            ("b".to_string(), vec!["a".to_string()]),
        ]
        .into();
        let tested = build_transitive_tested_set(&test_calls, &call_graph);
        let config = Config::default();

        let warnings =
            detect_untested_functions(&declared, &prod_calls, &tested, &[], &config);
        assert!(warnings.is_empty(), "cycle terminates; both a and b are tested");
    }

    #[test]
    fn test_untested_leaf_still_flagged() {
        // Test calls A, A calls B, but D is never called transitively → D flagged
        let declared = vec![
            make_declared("a", false),
            make_declared("b", false),
            make_declared("d", false),
        ];
        let prod_calls: HashSet<String> =
            ["a", "b", "d"].iter().map(|s| s.to_string()).collect();
        let test_calls: HashSet<String> = ["a".to_string()].into();
        let call_graph: HashMap<String, Vec<String>> =
            [("a".to_string(), vec!["b".to_string()])].into();
        let tested = build_transitive_tested_set(&test_calls, &call_graph);
        let config = Config::default();

        let warnings =
            detect_untested_functions(&declared, &prod_calls, &tested, &[], &config);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].function_name, "d");
    }

    #[test]
    fn test_empty_call_graph_falls_back_to_direct() {
        // Empty call graph → only directly tested functions are cleared
        let declared = vec![make_declared("a", false), make_declared("b", false)];
        let prod_calls: HashSet<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        let tested: HashSet<String> = ["a".to_string()].into();
        let config = Config::default();

        let warnings =
            detect_untested_functions(&declared, &prod_calls, &tested, &[], &config);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].function_name, "b");
    }
}
