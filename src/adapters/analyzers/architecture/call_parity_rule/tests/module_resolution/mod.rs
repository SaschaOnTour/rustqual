//! Workspace-level tests for module-path resolution as the call graph
//! canonicalises call sites and `use` imports. Each case sets up a
//! multi-file (or inline-mod) workspace, builds the call graph, and asserts
//! whether an import / call site resolves to the expected canonical path.
//! Split into focused sub-files (each ≤ the SRP file-length cap); the shared
//! `EdgeCase` table is partitioned across them and driven by `run_edge_cases`.

pub(super) use super::support::{build_workspace, callees_of, graph_3l, graph_contains_edge};

/// `(path, source)` file entries for a workspace fixture.
pub(super) type WsFiles = &'static [(&'static str, &'static str)];
/// Module-resolution edge case: `(label, files, caller, target, present)`
/// where `present` = the edge must exist.
pub(super) type EdgeCase = (&'static str, WsFiles, &'static str, &'static str, bool);

mod part_a;
mod part_b;

/// Build a workspace and its call graph under the three-layer layout.
pub(super) fn graph_3l_of(
    files: WsFiles,
) -> crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::CallGraph {
    graph_3l(&build_workspace(files))
}

/// Assert each `EdgeCase` resolves (or not) to its expected edge. Shared by the
/// partitioned case tables so neither part re-duplicates the loop body.
pub(super) fn run_edge_cases(cases: &[EdgeCase]) {
    for (label, files, caller, target, present) in cases {
        let graph = graph_3l_of(files);
        assert_eq!(
            graph_contains_edge(&graph, caller, target),
            *present,
            "case {label}: edge {caller} → {target}; callees: {:?}",
            callees_of(&graph, caller),
        );
    }
}
