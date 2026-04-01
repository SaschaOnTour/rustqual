// qual:allow(coupling) reason: "leaf analysis module — high instability is expected"
pub(crate) mod assertions;
pub(crate) mod coverage;
pub(crate) mod lcov;
pub(crate) mod sut;
pub(crate) mod untested;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use syn::visit::Visit;

use crate::analyzer::FunctionAnalysis;
use crate::config::Config;
use crate::dry::dead_code::DeadCodeWarning;
use crate::dry::DeclaredFunction;
use crate::scope::ProjectScope;

/// A single test quality warning.
#[derive(Debug, Clone)]
pub struct TqWarning {
    pub file: String,
    pub line: usize,
    pub function_name: String,
    pub kind: TqWarningKind,
    pub suppressed: bool,
}

/// The kind of test quality issue detected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TqWarningKind {
    /// TQ-001: Test function has no assertions.
    NoAssertion,
    /// TQ-002: Test function does not call any production function.
    NoSut,
    /// TQ-003: Production function is called from prod but never from any test.
    Untested,
    /// TQ-004: Production function has 0 execution count in LCOV data.
    Uncovered,
    /// TQ-005: Logic occurrence at a line that is uncovered in LCOV data.
    UntestedLogic { uncovered_lines: Vec<(String, usize)> },
}

/// Results of test quality analysis.
#[derive(Debug, Clone, Default)]
pub struct TqAnalysis {
    pub warnings: Vec<TqWarning>,
}

/// Input context for test quality analysis (bundles many params to stay under SRP threshold).
pub(crate) struct TqContext<'a> {
    pub parsed: &'a [(String, String, syn::File)],
    pub scope: &'a ProjectScope,
    pub config: &'a Config,
    pub declared_fns: &'a [DeclaredFunction],
    pub prod_calls: &'a HashSet<String>,
    pub test_calls: &'a HashSet<String>,
    pub all_results: &'a [FunctionAnalysis],
    pub dead_code: &'a [DeadCodeWarning],
    pub coverage_path: Option<&'a Path>,
}

/// Collects per-function call targets from ALL function bodies (including ignored functions).
/// Used to build a complete call graph for TQ transitive analysis.
#[derive(Default)]
struct FullCallGraphCollector {
    functions: Vec<(String, Vec<String>)>,
    current_fn: Option<String>,
    current_calls: Vec<String>,
}

impl<'ast> Visit<'ast> for FullCallGraphCollector {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let prev_fn = self.current_fn.take();
        let prev_calls = std::mem::take(&mut self.current_calls);
        let name = node.sig.ident.to_string();
        self.current_fn = Some(name.clone());
        syn::visit::visit_item_fn(self, node);
        self.functions
            .push((name, std::mem::take(&mut self.current_calls)));
        self.current_fn = prev_fn;
        self.current_calls = prev_calls;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let prev_fn = self.current_fn.take();
        let prev_calls = std::mem::take(&mut self.current_calls);
        let name = node.sig.ident.to_string();
        self.current_fn = Some(name.clone());
        syn::visit::visit_impl_item_fn(self, node);
        self.functions
            .push((name, std::mem::take(&mut self.current_calls)));
        self.current_fn = prev_fn;
        self.current_calls = prev_calls;
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if self.current_fn.is_some() {
            if let syn::Expr::Path(ref p) = *node.func {
                if let Some(last) = p.path.segments.last() {
                    self.current_calls.push(last.ident.to_string());
                }
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if self.current_fn.is_some() {
            self.current_calls.push(node.method.to_string());
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        // Catch function references passed as arguments (e.g. `.for_each(print_srp_section)`)
        if self.current_fn.is_some() {
            if let Some(last) = node.path.segments.last() {
                self.current_calls.push(last.ident.to_string());
            }
        }
        syn::visit::visit_expr_path(self, node);
    }
}

/// Build a per-function call graph from all parsed files, including ignored functions.
/// Operation: AST walking, no own calls.
pub(crate) fn build_full_call_graph(
    parsed: &[(String, String, syn::File)],
) -> HashMap<String, Vec<String>> {
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    for (_, _, syntax) in parsed {
        let mut collector = FullCallGraphCollector::default();
        collector.visit_file(syntax);
        for (name, calls) in collector.functions {
            graph.entry(name).or_default().extend(calls);
        }
    }
    graph
}

/// Build the set of function names that transitively reach production code.
/// Operation: reverse BFS from production function names through the call graph.
pub(crate) fn build_reaches_prod_set(
    call_graph: &HashMap<String, Vec<String>>,
    declared_fns: &[DeclaredFunction],
) -> HashSet<String> {
    // Build reverse graph: callee → [callers]
    let mut reverse: HashMap<&str, Vec<&str>> = HashMap::new();
    for (caller, callees) in call_graph {
        for callee in callees {
            reverse
                .entry(callee.as_str())
                .or_default()
                .push(caller.as_str());
        }
    }
    // Seed: all production function names
    let mut reaches: HashSet<String> = declared_fns
        .iter()
        .filter(|f| !f.is_test)
        .map(|f| f.name.clone())
        .collect();
    let mut queue: VecDeque<String> = reaches.iter().cloned().collect();
    // BFS backward: find all functions that transitively call prod functions
    while let Some(name) = queue.pop_front() {
        if let Some(callers) = reverse.get(name.as_str()) {
            callers.iter().for_each(|caller| {
                if reaches.insert(caller.to_string()) {
                    queue.push_back(caller.to_string());
                }
            });
        }
    }
    reaches
}

/// Analyze test quality across all parsed files.
/// Integration: orchestrates sub-detectors, no logic.
pub(crate) fn analyze_test_quality(ctx: &TqContext<'_>) -> TqAnalysis {
    let mut warnings = Vec::new();

    // Build complete call graph (includes ignored functions like visit_*)
    let full_graph = build_full_call_graph(ctx.parsed);
    let reaches_prod = build_reaches_prod_set(&full_graph, ctx.declared_fns);

    let assertion_free =
        assertions::detect_assertion_free_tests(ctx.parsed, &ctx.config.test.extra_assertion_macros);
    warnings.extend(assertion_free);

    let no_sut =
        sut::detect_no_sut_tests(ctx.parsed, ctx.scope, ctx.declared_fns, &reaches_prod);
    warnings.extend(no_sut);

    // Seed from test_calls + ignored functions (entry points, visitors are implicitly tested)
    let seed: HashSet<String> = ctx
        .test_calls
        .iter()
        .cloned()
        .chain(
            ctx.declared_fns
                .iter()
                .filter(|f| ctx.config.is_ignored_function(&f.name))
                .map(|f| f.name.clone()),
        )
        .collect();
    let transitive_tested = untested::build_transitive_tested_set(&seed, &full_graph);

    let untested_fns = untested::detect_untested_functions(
        ctx.declared_fns,
        ctx.prod_calls,
        &transitive_tested,
        ctx.dead_code,
        ctx.config,
    );
    warnings.extend(untested_fns);

    ctx.coverage_path
        .and_then(|p| lcov::parse_lcov(p).ok())
        .iter()
        .for_each(|lcov_data| {
            let uncovered = coverage::detect_uncovered_functions(ctx.all_results, lcov_data);
            let untested_logic = coverage::detect_untested_logic(ctx.all_results, lcov_data);
            warnings.extend(uncovered);
            warnings.extend(untested_logic);
        });

    TqAnalysis { warnings }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tq_analysis_default_empty() {
        let analysis = TqAnalysis::default();
        assert!(analysis.warnings.is_empty());
    }

    #[test]
    fn test_tq_warning_kind_equality() {
        assert_eq!(TqWarningKind::NoAssertion, TqWarningKind::NoAssertion);
        assert_eq!(TqWarningKind::NoSut, TqWarningKind::NoSut);
        assert_eq!(TqWarningKind::Untested, TqWarningKind::Untested);
        assert_eq!(TqWarningKind::Uncovered, TqWarningKind::Uncovered);
        assert_ne!(TqWarningKind::NoAssertion, TqWarningKind::NoSut);
    }
}
