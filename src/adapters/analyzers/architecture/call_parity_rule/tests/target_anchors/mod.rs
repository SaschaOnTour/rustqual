//! Tests for the target-anchor capability surface — the set of synthetic
//! `<Trait>::<method>` anchors that count as target-layer capabilities for
//! Check B/D coverage decisions. Anchors are capabilities (not concrete fns),
//! and adapter coverage of a `dyn Trait.method()` dispatch must be checked
//! against them, not against the concrete impls those dispatches reach. Split
//! into focused sub-files (each ≤ the SRP file-length cap); shared imports,
//! case-table types, and the `anchor_caps`/`graph_3l_of` helpers live here and
//! reach the sub-modules via `use super::*`.

pub(super) use super::support::{
    build_graph_only, build_workspace, callees_of, empty_cfg_test, graph_3l, graph_contains_edge,
    graph_of, three_layer,
};
pub(super) use std::collections::HashSet;

mod across_bound_spellings;
mod capability;
mod trait_anchor_edges;

/// `(path, source)` file entries for a small workspace fixture.
pub(super) type WsFiles = &'static [(&'static str, &'static str)];
/// Target-anchor capability case: `(label, files, adapters, anchor, present)`.
pub(super) type AnchorCapCase = (
    &'static str,
    WsFiles,
    &'static [&'static str],
    &'static str,
    bool,
);
/// Generic-dispatch trait-anchor edge case: `(label, files, caller, anchor)`.
pub(super) type EdgeCase = (&'static str, WsFiles, &'static str, &'static str);

/// Build a workspace and return the target-layer anchor capability names
/// for the `application` target with the given adapter set.
pub(super) fn anchor_caps(files: WsFiles, adapters: &[&str]) -> HashSet<String> {
    let owned: Vec<String> = adapters.iter().map(|s| s.to_string()).collect();
    graph_of(&build_workspace(files))
        .target_anchor_capabilities("application", &owned)
        .map(|(name, _)| name.to_string())
        .collect()
}

/// Build a workspace and its call graph under the `three_layer` layout —
/// the shared shape for the generic-param trait-anchor edge tests.
pub(super) fn graph_3l_of(
    files: WsFiles,
) -> crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::CallGraph {
    graph_3l(&build_workspace(files))
}
