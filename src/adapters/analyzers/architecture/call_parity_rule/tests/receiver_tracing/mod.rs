//! Workspace-level tests for method-call tracing through receivers,
//! locally-bound bindings, parameter-bound generics, async fn bodies,
//! generic-fn path calls, and inline attribute-decorated impl blocks. Each
//! case builds a multi-file workspace, produces the call graph through the
//! three-layer layout, and asserts the expected `crate::…::Type::method` (or
//! free-fn) edges all exist. Split into focused sub-files (each ≤ the SRP
//! file-length cap); the `EdgeCase` table is partitioned and driven by
//! `run_edge_cases`.

pub(super) use super::support::{build_workspace, callees_of, graph_3l, graph_contains_edge};

/// `(path, source)` file entries for a workspace fixture.
pub(super) type WsFiles = &'static [(&'static str, &'static str)];
/// Receiver-tracing case: `(label, files, edges)` — every `(from, to)` edge
/// must exist in the call graph.
pub(super) type EdgeCase = (
    &'static str,
    WsFiles,
    &'static [(&'static str, &'static str)],
);

mod part_a;
mod part_b;

/// Build a workspace and its call graph under the three-layer layout.
pub(super) fn graph_3l_of(
    files: WsFiles,
) -> crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::CallGraph {
    graph_3l(&build_workspace(files))
}

/// Assert every `(from, to)` edge of each case exists. Shared by the
/// partitioned tables so neither part re-duplicates the loop body.
pub(super) fn run_edge_cases(cases: &[EdgeCase]) {
    for (label, files, edges) in cases {
        let graph = graph_3l_of(files);
        for (from, to) in *edges {
            assert!(
                graph_contains_edge(&graph, from, to),
                "case {label}: edge {from} → {to} missing; {from} callees: {:?}",
                callees_of(&graph, from),
            );
        }
    }
}
