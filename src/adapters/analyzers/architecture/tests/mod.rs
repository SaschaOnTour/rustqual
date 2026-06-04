// Module-level integration tests for the Architecture analyzer.
// Matcher-specific unit tests live in matcher/tests/.

mod analyzer;
mod call_parity_bench;
mod call_parity_golden;
mod compiled;
mod explain;
mod forbidden_rule;
mod golden_examples;
mod layer_rule;
mod rendering;
mod trait_contract;

use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};

/// Assert `hits` holds exactly one `ForbiddenEdge` whose reason equals
/// `expected_reason` and whose imported path starts with `imported_prefix`.
/// Shared by the forbidden-rule unit tests and the golden-example snapshot,
/// whose forbidden-hit verification is otherwise identical.
pub(super) fn assert_single_forbidden_edge(
    hits: &[MatchLocation],
    expected_reason: &str,
    imported_prefix: &str,
) {
    assert_eq!(hits.len(), 1, "expected one forbidden hit: {hits:?}");
    match &hits[0].kind {
        ViolationKind::ForbiddenEdge {
            reason,
            imported_path,
        } => {
            assert_eq!(reason, expected_reason);
            assert!(
                imported_path.starts_with(imported_prefix),
                "imported_path = {imported_path:?}"
            );
        }
        other => panic!("unexpected kind: {other:?}"),
    }
}
