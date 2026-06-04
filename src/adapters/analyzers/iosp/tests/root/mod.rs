//! IOSP classification + own-call analysis tests, split into focused
//! sub-files (each ≤ the SRP file-length cap). Shared imports and the
//! parse/analyze/classify helpers live here and reach the sub-modules via
//! `use super::*`; per-test case tables live with their tests.

pub(super) use crate::adapters::analyzers::iosp::*;
pub(super) use crate::adapters::shared::project_scope::ProjectScope;
pub(super) use crate::config::Config;

mod analysis_structure;
mod classification_exact;
mod classification_predicate;
mod classify_basics;
mod complexity_and_metadata;
mod dispatch_and_getters;
mod is_test;
mod metrics_exact;
mod own_call_resolution;
mod own_calls_a;
mod own_calls_b;

/// Helper: parse code, build scope from it, analyze with default config.
pub(super) fn parse_and_analyze(code: &str) -> Vec<FunctionAnalysis> {
    let syntax = syn::parse_file(code).expect("Failed to parse test code");
    let scope_files = vec![("test.rs", &syntax)];
    let scope = ProjectScope::from_files(&scope_files);
    let config = Config::default();
    let analyzer = Analyzer::new(&config, &scope);
    let mut results = analyzer.analyze_file(&syntax, "test.rs");
    let parsed = vec![("test.rs".to_string(), code.to_string(), syntax)];
    let recursive_lines = crate::adapters::source::filesystem::collect_recursive_lines(&parsed);
    crate::app::warnings::apply_recursive_annotations(&mut results, &recursive_lines);
    crate::app::warnings::apply_leaf_reclassification(&mut results);
    results
}

/// Helper: parse code with a custom config.
pub(super) fn parse_and_analyze_with_config(code: &str, config: &Config) -> Vec<FunctionAnalysis> {
    let syntax = syn::parse_file(code).expect("Failed to parse test code");
    let scope_files = vec![("test.rs", &syntax)];
    let scope = ProjectScope::from_files(&scope_files);
    let analyzer = Analyzer::new(config, &scope);
    analyzer.analyze_file(&syntax, "test.rs")
}

// ---------------------------------------------------------------
// Classification Tests
// ---------------------------------------------------------------

/// Classify `fn_name` in `code` under the default config.
pub(super) fn classify(code: &str, fn_name: &str) -> Classification {
    parse_and_analyze(code)
        .into_iter()
        .find(|r| r.name == fn_name)
        .unwrap_or_else(|| panic!("fn {fn_name} not found"))
        .classification
}

/// Classify `fn_name` in `code` under a custom config.
pub(super) fn classify_cfg(code: &str, fn_name: &str, config: &Config) -> Classification {
    parse_and_analyze_with_config(code, config)
        .into_iter()
        .find(|r| r.name == fn_name)
        .unwrap_or_else(|| panic!("fn {fn_name} not found"))
        .classification
}

/// Whether `fn_name` in `code` is classified as test code (`is_test`).
pub(super) fn is_test_of(code: &str, fn_name: &str) -> bool {
    analysis_of(code, fn_name).is_test
}

/// Full analysis of function `name` in `code` (with leaf reclassification).
pub(super) fn analysis_of(code: &str, name: &str) -> FunctionAnalysis {
    parse_and_analyze(code)
        .into_iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("fn {name} not found"))
}

/// Raw analysis of `name` without leaf reclassification, so a genuine Violation
/// keeps its `Violation` classification (and its `effort_score`/`severity`).
pub(super) fn raw_analysis_of(code: &str, name: &str) -> FunctionAnalysis {
    parse_and_analyze_with_config(code, &Config::default())
        .into_iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("fn {name} not found"))
}

/// Complexity metrics of the single function `f` in `code`.
pub(super) fn metrics_of(code: &str) -> ComplexityMetrics {
    analysis_of(code, "f")
        .complexity
        .expect("f should have complexity metrics")
}

/// Own-call targets recorded for function `name` in `code`.
pub(super) fn own_calls_of(code: &str, name: &str) -> Vec<String> {
    analysis_of(code, name).own_calls
}

/// Names of all analysed functions in `code`.
pub(super) fn analysed_names(code: &str) -> Vec<String> {
    parse_and_analyze(code)
        .into_iter()
        .map(|r| r.name)
        .collect()
}

// (label, code, fn_name, expected) — Integration = orchestrates own calls only;
// Operation = contains logic and no own calls; Trivial = empty / single-return /
// getter. Own calls include self-methods, free fns, and same-type path-ctors;
// stdlib method/path calls (Vec::is_empty, String::new) are NOT own calls.
