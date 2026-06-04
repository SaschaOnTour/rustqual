//! Tests for `app::warnings` — rust-allow counting, extended-warning flag
//! application, test-violation exclusion, and orphan-suppression detection.
//! Split into focused sub-files (each ≤ the SRP file-length cap); shared
//! imports + the apply_warnings / *_orphan_count helpers live here (the
//! orphan finding-builders live in `orphan_support`, re-exported below) and
//! reach the sub-modules via `use super::*`.

pub(super) use crate::adapters::analyzers::iosp::{
    Classification, ComplexityMetrics, FunctionAnalysis,
};
pub(super) use crate::app::warnings::*;
pub(super) use crate::config::Config;
pub(super) use crate::report::Summary;
pub(super) use std::collections::{HashMap, HashSet};

mod complexity_flags;
mod exclude_and_error_handling;
mod extended_warnings;
mod flag_application;
mod orphan_basics_and_suppression;
mod orphan_complexity_thresholds;
mod orphan_dry_and_disabled;
mod orphan_mod_internals;
mod orphan_module_tq_arch;
mod orphan_support;
pub(super) use orphan_support::*;

// ── apply_extended_warnings ───────────────────────────────────

pub(super) use crate::adapters::analyzers::iosp::compute_severity;

pub(super) fn make_func_with_metrics(metrics: ComplexityMetrics) -> FunctionAnalysis {
    let severity = compute_severity(&Classification::Operation);
    FunctionAnalysis {
        name: "test_fn".to_string(),
        file: "test.rs".to_string(),
        line: 1,
        classification: Classification::Operation,
        parent_type: None,
        suppressed: false,
        complexity: Some(metrics),
        qualified_name: "test_fn".to_string(),
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
        effort_score: None,
        is_test: false,
    }
}

/// Run `apply_extended_warnings` over a single function and return the
/// (possibly flag-mutated) function plus the populated summary.
pub(super) fn apply_warnings(
    func: FunctionAnalysis,
    config: &Config,
    unsafe_lines: &HashMap<String, HashSet<usize>>,
) -> (FunctionAnalysis, Summary) {
    let mut summary = Summary::default();
    let mut results = vec![func];
    apply_extended_warnings(&mut results, config, &mut summary, unsafe_lines);
    (results.into_iter().next().unwrap(), summary)
}

pub(super) type FlagFn = fn(&FunctionAnalysis) -> bool;
pub(super) type CountFn = fn(&Summary) -> usize;

// ── Regression tests: no false-positive orphans when the marker ──
// ── clears warning flags via suppression.                       ──
//
// These tests reproduce the Bug 3 iteration where my first orphan
// checker read `fa.cognitive_warning` and friends — flags that
// `apply_file_suppressions` clears when `// qual:allow(complexity)`
// matches. The checker then saw no position and flagged the marker
// as orphan, even though it was actively doing its job. The fixed
// checker reads raw `complexity` metrics against config thresholds,
// independent of the suppression flags.

pub(super) fn make_fa_with_complexity(
    file: &str,
    line: usize,
    metrics: crate::adapters::analyzers::iosp::ComplexityMetrics,
) -> FunctionAnalysis {
    FunctionAnalysis {
        name: "f".into(),
        qualified_name: "f".into(),
        file: file.into(),
        line,
        classification: Classification::Operation,
        parent_type: None,
        suppressed: false,
        complexity: Some(metrics),
        severity: None,
        cognitive_warning: false,
        cyclomatic_warning: false,
        nesting_depth_warning: false,
        function_length_warning: false,
        unsafe_warning: false,
        error_handling_warning: false,
        complexity_suppressed: true,
        own_calls: vec![],
        parameter_count: 0,
        is_trait_impl: false,
        is_test: false,
        effort_score: None,
    }
}

/// Run the orphan detector for a single `qual:allow(complexity)` marker at
/// `sup_line` against a function at `fn_line` carrying `metrics`.
pub(super) fn complexity_sup_orphans(
    sup_line: usize,
    fn_line: usize,
    metrics: crate::adapters::analyzers::iosp::ComplexityMetrics,
) -> Vec<crate::domain::findings::OrphanSuppression> {
    let mut sups = HashMap::new();
    sups.insert(
        "src/x.rs".to_string(),
        vec![crate::findings::Suppression {
            line: sup_line,
            dimensions: vec![crate::findings::Dimension::Complexity],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis.results = vec![make_fa_with_complexity("src/x.rs", fn_line, metrics)];
    crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &std::collections::HashMap::new(),
        &analysis,
        &Config::default(),
    )
}
