//! Evaluation tests for the call_parity gaps surfaced in rustqual
//! 1.2.3 by the rlm post-1.2.3-install audit.
//!
//! Two bug classes from the user's reduction (2026-05-14):
//! - **Class 1** (bug4 method-call) — already covered in
//!   rlm_v122_eval.rs as sister tests under the bug4 family.
//! - **Class 2** (bug5) — concrete UFCS into a sibling submodule
//!   (`Type::new(args)` where `Type` lives in `mod sibling` and
//!   is named-imported at the call site) is not traced. Confirmed
//!   pre-existing across 1.2.2 + 1.2.3; newly visible in 1.2.3
//!   because the Bug-2 `pub use` fix made enclosing generic
//!   dispatchers reachable, surfacing the latent inner gap.

use super::support::{build_graph_only, build_workspace, empty_cfg_test, three_layer};
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::CallGraph;
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use std::collections::HashSet;

fn graph_contains_edge(graph: &CallGraph, from: &str, to: &str) -> bool {
    graph
        .forward
        .get(from)
        .is_some_and(|callees| callees.contains(to))
}

fn callees_of<'g>(graph: &'g CallGraph, from: &str) -> Vec<&'g str> {
    graph
        .forward
        .get(from)
        .map(|set| {
            let mut v: Vec<&str> = set.iter().map(String::as_str).collect();
            v.sort();
            v
        })
        .unwrap_or_default()
}

fn rlm_layers_for_eval() -> LayerDefinitions {
    three_layer()
}

// ═══════════════════════════════════════════════════════════════════
// Bug 5 (v1.2.4) — concrete UFCS into sibling submodule
// ═══════════════════════════════════════════════════════════════════
//
// `ConcreteOutput::new(args)` where `ConcreteOutput` is imported via
// `use submodule::ConcreteOutput;` (relative sibling-submodule) is
// not traced. Same-file V1.0 control IS traced. User-bisected:
// minimal trigger is JUST the submodule split — no match, no
// destructure, no helpers, no T-substitution required.
//
// Mirrors rlm's exact shape:
//   application::middleware::response::OperationResponse
//   application::middleware::savings_recorder::record_operation<T>

#[test]
fn bug5_concrete_ufcs_in_sibling_submodule_traces_edge() {
    // Verbatim from /tmp/bug2-class2-repro/src/{application/mod,
    // application/response,cli/mod}.rs.
    let ws = build_workspace(&[
        (
            "src/application/mod.rs",
            r#"
            //! Class 2 repro — MINIMAL form.

            pub mod response;

            use serde::Serialize;

            use response::ConcreteOutput;

            pub fn dispatcher<T: Serialize>(value: &T) -> ConcreteOutput {
                let body = serde_json::to_string(value).unwrap_or_default();
                ConcreteOutput::new(body)
            }
            "#,
        ),
        (
            "src/application/response.rs",
            r#"
            //! Sibling submodule with the concrete type and its inherent `new`.

            pub struct ConcreteOutput {
                pub body: String,
            }

            impl ConcreteOutput {
                pub fn new(body: String) -> Self {
                    Self { body }
                }
            }
            "#,
        ),
        (
            "src/cli/mod.rs",
            r#"
            //! Adapter — V1.5.

            use crate::application::dispatcher;

            pub fn cmd_run() -> String {
                let out = dispatcher(&42u32);
                out.body
            }
            "#,
        ),
    ]);
    let graph = build_graph_only(
        &ws,
        &rlm_layers_for_eval(),
        &empty_cfg_test(),
        &HashSet::new(),
    );
    let dispatcher = "crate::application::dispatcher";
    let new_target = "crate::application::response::ConcreteOutput::new";
    assert!(
        graph_contains_edge(&graph, dispatcher, new_target),
        "concrete UFCS `ConcreteOutput::new(body)` inside generic \
         `dispatcher<T>` body must emit edge to \
         `{new_target}` even when `ConcreteOutput` is named-imported \
         from a sibling submodule. dispatcher callees: {:?}",
        callees_of(&graph, dispatcher),
    );
}
