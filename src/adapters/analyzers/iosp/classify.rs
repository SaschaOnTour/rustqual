use syn::visit::Visit;
use syn::ItemImpl;

use crate::adapters::analyzers::iosp::scope::ProjectScope;
use crate::config::Config;

use super::types::{CallOccurrence, Classification, ComplexityMetrics, LogicOccurrence};
use super::visitor::BodyVisitor;

/// Check if a body is trivial (empty body only — no logic, no calls).
/// Single-statement bodies are now analyzed by the BodyVisitor (A5).
/// Trivial: returns empty-body result.
pub(crate) fn is_trivial_body(
    body: &syn::Block,
) -> Option<(Classification, Option<ComplexityMetrics>, Vec<String>)> {
    body.stmts
        .is_empty()
        .then_some((Classification::Trivial, None, vec![]))
}

/// Deduplicate call occurrences, keeping only the first occurrence per name.
pub(crate) fn dedup_calls(calls: Vec<CallOccurrence>) -> Vec<CallOccurrence> {
    let mut seen = std::collections::HashSet::new();
    calls
        .into_iter()
        .filter(|c| seen.insert(c.name.clone()))
        .collect()
}

/// Classify based on collected logic and call findings.
/// Operation: pure logic (match on booleans).
pub(crate) fn classify_from_findings(
    logic: Vec<LogicOccurrence>,
    own_calls: Vec<CallOccurrence>,
) -> Classification {
    let has_logic = !logic.is_empty();
    let has_own_calls = !own_calls.is_empty();

    match (has_logic, has_own_calls) {
        (false, true) => Classification::Integration,
        (true, false) => Classification::Operation,
        (false, false) => Classification::Trivial,
        (true, true) => Classification::Violation {
            has_logic,
            has_own_calls,
            logic_locations: logic,
            call_locations: own_calls,
        },
    }
}

/// Build classification result from a BodyVisitor's collected data.
/// Integration: orchestrates classify_from_findings, dedup_calls, attach_metrics.
pub(crate) fn build_classification_result(
    logic: Vec<LogicOccurrence>,
    own_calls: Vec<CallOccurrence>,
    metrics: ComplexityMetrics,
) -> (Classification, Option<ComplexityMetrics>, Vec<String>) {
    let deduped = dedup_calls(own_calls);
    let call_names: Vec<String> = deduped.iter().map(|c| c.name.clone()).collect();
    let classification = classify_from_findings(logic, deduped);
    let (class, metrics) = attach_metrics(classification, metrics);
    (class, metrics, call_names)
}

/// Pair a classification with optional metrics (None for Trivial).
/// Operation: match logic.
pub(crate) fn attach_metrics(
    classification: Classification,
    metrics: ComplexityMetrics,
) -> (Classification, Option<ComplexityMetrics>) {
    if matches!(classification, Classification::Trivial) {
        (classification, None)
    } else {
        (classification, Some(metrics))
    }
}

/// Analyze a single function body and classify it.
/// Integration: orchestrates is_trivial_body, BodyVisitor, build_classification_result.
pub fn classify_function(
    body: &syn::Block,
    config: &Config,
    scope: &ProjectScope,
    fn_name: &str,
    type_context: (Option<&str>, &syn::Signature),
) -> (Classification, Option<ComplexityMetrics>, Vec<String>) {
    is_trivial_body(body).unwrap_or_else(|| {
        let param_types = crate::adapters::analyzers::iosp::extract_param_types(type_context.1);
        let mut visitor =
            BodyVisitor::new(config, scope, Some(fn_name), type_context.0, param_types);
        body.stmts.iter().for_each(|stmt| visitor.visit_stmt(stmt));
        let logic = visitor.logic;
        let own_calls = visitor.own_calls;
        let open_line = body.brace_token.span.open().start().line;
        let close_line = body.brace_token.span.close().end().line;
        let function_lines = close_line.saturating_sub(open_line) + 1;
        let logic_occurrences: Vec<LogicOccurrence> = logic
            .iter()
            .map(|lo| LogicOccurrence {
                kind: lo.kind.clone(),
                line: lo.line,
            })
            .collect();
        let metrics = ComplexityMetrics {
            logic_count: logic.len(),
            call_count: own_calls.len(),
            max_nesting: visitor.max_nesting,
            cognitive_complexity: visitor.cognitive_complexity,
            cyclomatic_complexity: visitor.cyclomatic_complexity,
            hotspots: visitor.complexity_hotspots,
            magic_numbers: visitor.magic_numbers,
            function_lines,
            unsafe_blocks: visitor.unsafe_block_count,
            unwrap_count: visitor.unwrap_count,
            expect_count: visitor.expect_count,
            panic_count: visitor.panic_count,
            todo_count: visitor.todo_count,
            logic_occurrences,
        };
        build_classification_result(logic, own_calls, metrics)
    })
}

/// Extract the type name from an impl block, including generic parameters.
/// Operation: if-let logic.
pub(crate) fn extract_type_name(item_impl: &ItemImpl) -> Option<String> {
    if let syn::Type::Path(tp) = &*item_impl.self_ty {
        tp.path.segments.last().map(|s| {
            let name = s.ident.to_string();
            match &s.arguments {
                syn::PathArguments::AngleBracketed(args) => {
                    let params: Vec<String> = args
                        .args
                        .iter()
                        .map(|a| quote::quote!(#a).to_string())
                        .collect();
                    format!("{name}<{}>", params.join(","))
                }
                _ => name,
            }
        })
    } else {
        None
    }
}
