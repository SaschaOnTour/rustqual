mod cycles;
mod graph;
pub(crate) mod metrics;
pub(crate) mod sdp;

/// Module-level dependency graph.
#[derive(Debug, Clone)]
pub struct ModuleGraph {
    /// Module names, indexed by position.
    pub modules: Vec<String>,
    /// Forward adjacency list: module[i] depends on modules in forward[i].
    pub forward: Vec<Vec<usize>>,
}

/// Coupling metrics for a single module.
#[derive(Debug, Clone)]
pub struct CouplingMetrics {
    /// Module name.
    pub module_name: String,
    /// Afferent coupling (Ca): number of modules that depend on this one.
    pub afferent: usize,
    /// Efferent coupling (Ce): number of modules this one depends on.
    pub efferent: usize,
    /// Instability: Ce / (Ca + Ce). Range [0.0, 1.0].
    /// 0.0 = maximally stable, 1.0 = maximally unstable.
    pub instability: f64,
    /// Names of modules that depend on this one (incoming).
    pub incoming: Vec<String>,
    /// Names of modules this one depends on (outgoing).
    pub outgoing: Vec<String>,
    /// Whether this module's coupling warnings are suppressed via `// qual:allow(coupling)`.
    pub suppressed: bool,
    /// Whether this module exceeds coupling thresholds (set by pipeline).
    pub warning: bool,
}

/// A strongly connected component (cycle) in the module graph.
#[derive(Debug, Clone)]
pub struct CycleReport {
    /// Module names in the cycle (sorted alphabetically).
    pub modules: Vec<String>,
}

/// Full result of coupling analysis.
#[derive(Debug, Clone)]
pub struct CouplingAnalysis {
    /// Per-module coupling metrics.
    pub metrics: Vec<CouplingMetrics>,
    /// Circular dependencies (SCCs with 2+ modules).
    pub cycles: Vec<CycleReport>,
    /// Stable Dependencies Principle violations.
    pub sdp_violations: Vec<sdp::SdpViolation>,
}

/// Run coupling analysis on parsed files.
/// Integration: orchestrates graph building, metrics computation, and cycle detection.
pub fn analyze_coupling(parsed: &[(String, String, syn::File)]) -> CouplingAnalysis {
    let graph = graph::build_module_graph(parsed);
    let metrics = metrics::compute_coupling_metrics(&graph);
    let cycles = cycles::detect_cycles(&graph);
    let sdp_violations = sdp::check_sdp(&graph, &metrics);
    CouplingAnalysis {
        metrics,
        cycles,
        sdp_violations,
    }
}

/// Convert a file path to its top-level module name.
/// Operation: string manipulation logic, no own calls.
///
/// Examples:
/// - `main.rs` → `main`
/// - `config/mod.rs` → `config`
/// - `analyzer/types.rs` → `analyzer`
/// - `src/pipeline.rs` → `pipeline`
pub fn file_to_module(file_path: &str) -> String {
    let path = file_path.replace('\\', "/");
    let stripped = path.strip_prefix("src/").unwrap_or(&path);
    if let Some(slash_pos) = stripped.find('/') {
        stripped[..slash_pos].to_string()
    } else {
        stripped.strip_suffix(".rs").unwrap_or(stripped).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_code(code: &str) -> syn::File {
        syn::parse_file(code).expect("Failed to parse test code")
    }

    fn make_parsed(files: Vec<(&str, &str)>) -> Vec<(String, String, syn::File)> {
        files
            .into_iter()
            .map(|(path, code)| (path.to_string(), code.to_string(), parse_code(code)))
            .collect()
    }

    /// Find module index by name in graph.modules.
    fn idx(graph: &ModuleGraph, name: &str) -> usize {
        graph
            .modules
            .iter()
            .position(|m| m == name)
            .unwrap_or_else(|| panic!("module '{name}' not found in graph"))
    }

    // ── file_to_module tests ────────────────────────────────────────

    #[test]
    fn test_file_to_module_root_file() {
        assert_eq!(file_to_module("main.rs"), "main");
        assert_eq!(file_to_module("pipeline.rs"), "pipeline");
    }

    #[test]
    fn test_file_to_module_subdir_mod() {
        assert_eq!(file_to_module("config/mod.rs"), "config");
        assert_eq!(file_to_module("analyzer/mod.rs"), "analyzer");
    }

    #[test]
    fn test_file_to_module_subdir_file() {
        assert_eq!(file_to_module("analyzer/types.rs"), "analyzer");
        assert_eq!(file_to_module("report/text.rs"), "report");
    }

    #[test]
    fn test_file_to_module_src_prefix() {
        assert_eq!(file_to_module("src/main.rs"), "main");
        assert_eq!(file_to_module("src/config/mod.rs"), "config");
        assert_eq!(file_to_module("src/analyzer/types.rs"), "analyzer");
    }

    #[test]
    fn test_file_to_module_backslash() {
        assert_eq!(file_to_module("src\\config\\mod.rs"), "config");
        assert_eq!(file_to_module("analyzer\\types.rs"), "analyzer");
    }

    // ── build_module_graph tests ────────────────────────────────────

    #[test]
    fn test_build_graph_no_deps() {
        let parsed = make_parsed(vec![
            ("main.rs", "fn main() {}"),
            ("config.rs", "pub struct Config;"),
        ]);
        let graph = graph::build_module_graph(&parsed);
        assert_eq!(graph.modules.len(), 2);
        assert!(graph.forward.iter().all(|adj| adj.is_empty()));
    }

    #[test]
    fn test_build_graph_simple_dep() {
        let parsed = make_parsed(vec![
            ("main.rs", "use crate::config::Config; fn main() {}"),
            ("config.rs", "pub struct Config;"),
        ]);
        let graph = graph::build_module_graph(&parsed);
        let main_idx = idx(&graph, "main");
        let config_idx = idx(&graph, "config");
        assert!(graph.forward[main_idx].contains(&config_idx));
        assert!(graph.forward[config_idx].is_empty());
    }

    #[test]
    fn test_build_graph_self_dep_skipped() {
        let parsed = make_parsed(vec![
            (
                "analyzer/mod.rs",
                "use crate::analyzer::types::Foo; fn f() {}",
            ),
            ("analyzer/types.rs", "pub struct Foo;"),
        ]);
        let graph = graph::build_module_graph(&parsed);
        let analyzer_idx = idx(&graph, "analyzer");
        assert!(
            graph.forward[analyzer_idx].is_empty(),
            "Self-dependencies should be skipped"
        );
    }

    #[test]
    fn test_build_graph_group_use() {
        let parsed = make_parsed(vec![
            (
                "main.rs",
                "use crate::{config::Config, pipeline::run}; fn main() {}",
            ),
            ("config.rs", "pub struct Config;"),
            ("pipeline.rs", "pub fn run() {}"),
        ]);
        let graph = graph::build_module_graph(&parsed);
        let main_idx = idx(&graph, "main");
        assert_eq!(graph.forward[main_idx].len(), 2);
    }

    #[test]
    fn test_build_graph_external_dep_ignored() {
        let parsed = make_parsed(vec![(
            "main.rs",
            "use std::collections::HashMap; use serde::Deserialize; fn main() {}",
        )]);
        let graph = graph::build_module_graph(&parsed);
        let main_idx = idx(&graph, "main");
        assert!(
            graph.forward[main_idx].is_empty(),
            "External dependencies should be ignored"
        );
    }

    #[test]
    fn test_build_graph_multiple_files_same_module() {
        let parsed = make_parsed(vec![
            (
                "config/mod.rs",
                "use crate::analyzer::Foo; pub mod sections;",
            ),
            ("config/sections.rs", "pub struct Defaults;"),
            ("analyzer.rs", "pub struct Foo;"),
        ]);
        let graph = graph::build_module_graph(&parsed);
        let config_idx = idx(&graph, "config");
        let analyzer_idx = idx(&graph, "analyzer");
        assert!(graph.forward[config_idx].contains(&analyzer_idx));
    }

    #[test]
    fn test_build_graph_glob_use() {
        let parsed = make_parsed(vec![
            ("main.rs", "use crate::analyzer::*; fn main() {}"),
            ("analyzer.rs", "pub fn analyze() {}"),
        ]);
        let graph = graph::build_module_graph(&parsed);
        let main_idx = idx(&graph, "main");
        let analyzer_idx = idx(&graph, "analyzer");
        assert!(graph.forward[main_idx].contains(&analyzer_idx));
    }

    #[test]
    fn test_build_graph_rename_use() {
        let parsed = make_parsed(vec![
            ("main.rs", "use crate::config::Config as Cfg; fn main() {}"),
            ("config.rs", "pub struct Config;"),
        ]);
        let graph = graph::build_module_graph(&parsed);
        let main_idx = idx(&graph, "main");
        let config_idx = idx(&graph, "config");
        assert!(graph.forward[main_idx].contains(&config_idx));
    }

    // ── compute_coupling_metrics tests ──────────────────────────────

    #[test]
    fn test_metrics_empty() {
        let graph = ModuleGraph {
            modules: vec![],
            forward: vec![],
        };
        let metrics = metrics::compute_coupling_metrics(&graph);
        assert!(metrics.is_empty());
    }

    #[test]
    fn test_metrics_simple_dep() {
        // A → B
        let graph = ModuleGraph {
            modules: vec!["a".into(), "b".into()],
            forward: vec![vec![1], vec![]],
        };
        let metrics = metrics::compute_coupling_metrics(&graph);
        // A: Ca=0, Ce=1
        assert_eq!(metrics[0].afferent, 0);
        assert_eq!(metrics[0].efferent, 1);
        // B: Ca=1, Ce=0
        assert_eq!(metrics[1].afferent, 1);
        assert_eq!(metrics[1].efferent, 0);
    }

    #[test]
    fn test_metrics_instability_formula() {
        // A → B, A → C (Ce=2)
        let graph = ModuleGraph {
            modules: vec!["a".into(), "b".into(), "c".into()],
            forward: vec![vec![1, 2], vec![], vec![]],
        };
        let metrics = metrics::compute_coupling_metrics(&graph);
        // A: Ca=0, Ce=2, I = 2/(0+2) = 1.0
        assert!((metrics[0].instability - 1.0).abs() < f64::EPSILON);
        // B: Ca=1, Ce=0, I = 0/(1+0) = 0.0
        assert!((metrics[1].instability).abs() < f64::EPSILON);
    }

    #[test]
    fn test_metrics_isolated_module() {
        let graph = ModuleGraph {
            modules: vec!["isolated".into()],
            forward: vec![vec![]],
        };
        let metrics = metrics::compute_coupling_metrics(&graph);
        assert_eq!(metrics[0].afferent, 0);
        assert_eq!(metrics[0].efferent, 0);
        assert!((metrics[0].instability).abs() < f64::EPSILON);
    }

    // ── detect_cycles tests ─────────────────────────────────────────

    #[test]
    fn test_cycles_empty_graph() {
        let graph = ModuleGraph {
            modules: vec![],
            forward: vec![],
        };
        let cycles = cycles::detect_cycles(&graph);
        assert!(cycles.is_empty());
    }

    #[test]
    fn test_cycles_no_cycles() {
        // A → B → C (linear, no cycles)
        let graph = ModuleGraph {
            modules: vec!["a".into(), "b".into(), "c".into()],
            forward: vec![vec![1], vec![2], vec![]],
        };
        let cycles = cycles::detect_cycles(&graph);
        assert!(cycles.is_empty());
    }

    #[test]
    fn test_cycles_simple_cycle() {
        // A → B → A
        let graph = ModuleGraph {
            modules: vec!["a".into(), "b".into()],
            forward: vec![vec![1], vec![0]],
        };
        let cycles = cycles::detect_cycles(&graph);
        assert_eq!(cycles.len(), 1);
        assert!(cycles[0].modules.contains(&"a".to_string()));
        assert!(cycles[0].modules.contains(&"b".to_string()));
    }

    #[test]
    fn test_cycles_complex_cycle() {
        // A → B → C → A (3-node cycle)
        let graph = ModuleGraph {
            modules: vec!["a".into(), "b".into(), "c".into()],
            forward: vec![vec![1], vec![2], vec![0]],
        };
        let cycles = cycles::detect_cycles(&graph);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].modules.len(), 3);
    }

    #[test]
    fn test_cycles_self_loop_not_counted() {
        // A → A (self-loop, not a meaningful cycle)
        let graph = ModuleGraph {
            modules: vec!["a".into()],
            forward: vec![vec![0]],
        };
        let cycles = cycles::detect_cycles(&graph);
        assert!(
            cycles.is_empty(),
            "Self-loops should not be reported as cycles"
        );
    }

    #[test]
    fn test_cycles_two_independent_cycles() {
        // A ↔ B, C ↔ D (two separate cycles)
        let graph = ModuleGraph {
            modules: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            forward: vec![vec![1], vec![0], vec![3], vec![2]],
        };
        let cycles = cycles::detect_cycles(&graph);
        assert_eq!(cycles.len(), 2);
    }

    // ── analyze_coupling integration test ───────────────────────────

    #[test]
    fn test_analyze_coupling_integration() {
        let parsed = make_parsed(vec![
            ("main.rs", "use crate::config::Config; fn main() {}"),
            ("config.rs", "pub struct Config;"),
            ("pipeline.rs", "use crate::config::Config; pub fn run() {}"),
        ]);
        let analysis = analyze_coupling(&parsed);
        assert_eq!(analysis.metrics.len(), 3);
        assert!(analysis.cycles.is_empty());

        // config should have highest afferent coupling (2 dependents)
        let config_metrics = analysis
            .metrics
            .iter()
            .find(|m| m.module_name == "config")
            .unwrap();
        assert_eq!(config_metrics.afferent, 2);
        assert_eq!(config_metrics.efferent, 0);
    }

    #[test]
    fn test_analyze_coupling_with_cycle() {
        let parsed = make_parsed(vec![
            ("a.rs", "use crate::b::Foo; pub struct Bar;"),
            ("b.rs", "use crate::a::Bar; pub struct Foo;"),
        ]);
        let analysis = analyze_coupling(&parsed);
        assert_eq!(analysis.cycles.len(), 1);
        assert!(analysis.cycles[0].modules.contains(&"a".to_string()));
        assert!(analysis.cycles[0].modules.contains(&"b".to_string()));
    }

    #[test]
    fn test_analyze_coupling_no_crate_deps() {
        let parsed = make_parsed(vec![
            ("a.rs", "use std::collections::HashMap; fn f() {}"),
            ("b.rs", "use serde::Deserialize; fn g() {}"),
        ]);
        let analysis = analyze_coupling(&parsed);
        assert!(analysis.cycles.is_empty());
        for m in &analysis.metrics {
            assert_eq!(m.afferent, 0);
            assert_eq!(m.efferent, 0);
        }
    }
}
