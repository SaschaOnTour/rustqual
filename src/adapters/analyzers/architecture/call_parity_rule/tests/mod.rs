mod calls;
mod cfg_test_impl;
mod check_a;
mod check_b;
mod check_c;
mod check_d;
mod collect_findings_integration;
mod collectors_internal;
mod end_to_end_snapshot;
mod hint;
mod module_resolution;
mod peel_internal;
mod predicates_internal;
mod pub_fns;
mod reachable_targets;
mod receiver_tracing;
mod reexport_resolution;
mod reexports_internal;
mod regressions;
mod support;
mod target_anchors;
mod touchpoints;
mod workspace_graph_internals;

use std::collections::HashSet;

use super::local_symbols::collect_local_symbols_scoped;

/// The flat name table several of these tests build their fixtures from:
/// every name with at least one top-level declaration scope. Production uses
/// the scoped form only — exposing the flat one there let the legacy resolution
/// path produce bogus `crate::<file>::Inner` paths for inner-module names —
/// so this lives with the tests that need it, not in `local_symbols`.
pub(crate) fn collect_local_symbols(ast: &syn::File) -> HashSet<String> {
    collect_local_symbols_scoped(ast)
        .by_name
        .into_iter()
        .filter_map(|(name, scopes)| scopes.iter().any(|p| p.is_empty()).then_some(name))
        .collect()
}
