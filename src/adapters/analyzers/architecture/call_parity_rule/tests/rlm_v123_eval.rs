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

// ═══════════════════════════════════════════════════════════════════
// Bug 6 (v1.2.4) — sibling submodule wins over crate-root with same name
// ═══════════════════════════════════════════════════════════════════
//
// Rust 2018+ resolution order: `use foo::X` inside `crate::application`
// resolves to `crate::application::foo::X` (local sibling) when that
// submodule exists, even if `crate::foo` ALSO exists as a crate-root
// module with the same leaf name. Codex P2 round 7 (2026-05-14)
// flagged that `normalize_after_alias` checked `crate_root_modules`
// before the sibling-submodule branch, mis-routing a valid local
// import to the crate-root canonical and dropping the downstream
// edge.

#[test]
fn bug6_sibling_submodule_wins_over_crate_root_with_same_name() {
    // Workspace shape:
    //   src/response/mod.rs        ← crate-root `response`
    //   src/application/mod.rs     ← uses `response::Local::run()`
    //   src/application/response/  ← sibling `application::response`
    //   src/cli/mod.rs             ← reaches application::dispatch
    //
    // Expected: dispatch edge to `crate::application::response::Local::run`
    // (local sibling), NOT `crate::response::Local::run`.
    let ws = build_workspace(&[
        (
            "src/response/mod.rs",
            r#"
            pub struct OuterRoot;
            impl OuterRoot {
                pub fn run() {}
            }
            "#,
        ),
        (
            "src/application/response.rs",
            r#"
            pub struct Local;
            impl Local {
                pub fn run() {}
            }
            "#,
        ),
        (
            "src/application/mod.rs",
            r#"
            pub mod response;

            use response::Local;

            pub fn dispatch() {
                Local::run();
            }
            "#,
        ),
        (
            "src/cli/mod.rs",
            r#"
            use crate::application::dispatch;

            pub fn cmd_run() {
                dispatch();
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
    let dispatch = "crate::application::dispatch";
    let local_target = "crate::application::response::Local::run";
    let crate_root_target = "crate::response::OuterRoot::run";
    assert!(
        graph_contains_edge(&graph, dispatch, local_target),
        "Rust 2018+: `use response::Local` in `application/mod.rs` must \
         resolve to local sibling `crate::application::response::Local`, \
         not crate-root `crate::response`. dispatch callees: {:?}",
        callees_of(&graph, dispatch),
    );
    assert!(
        !graph_contains_edge(&graph, dispatch, crate_root_target),
        "must NOT route to crate-root `{crate_root_target}` — that's \
         the wrong target. dispatch callees: {:?}",
        callees_of(&graph, dispatch),
    );
}

// ═══════════════════════════════════════════════════════════════════
// Bug 7 (v1.2.4) — sibling submodule via INLINE mod declaration
// ═══════════════════════════════════════════════════════════════════
//
// `collect_workspace_module_paths` originally derived module paths
// from FILE paths only — inline `mod foo { ... }` blocks were
// invisible to the sibling-submodule discriminator. Codex P2 round 7
// (2026-05-14) flagged that a workspace shape like
// `mod outer { pub mod inner { ... } use inner::X; }` would never
// match `[outer, inner]` in the lookup set, dropping the local edge.

#[test]
fn bug7_inline_mod_sibling_import_traces_edge() {
    // Single file with inline mod hierarchy — no separate response.rs.
    // The `use inner::Local;` reference inside `mod outer` must route
    // to `crate::application::outer::inner::Local`, not stay relative.
    let ws = build_workspace(&[
        (
            "src/application/mod.rs",
            r#"
            pub mod outer {
                pub mod inner {
                    pub struct Local;
                    impl Local {
                        pub fn run() {}
                    }
                }

                use inner::Local;

                pub fn dispatch() {
                    Local::run();
                }
            }
            "#,
        ),
        (
            "src/cli/mod.rs",
            r#"
            use crate::application::outer::dispatch;

            pub fn cmd_run() {
                dispatch();
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
    let dispatch = "crate::application::outer::dispatch";
    let target = "crate::application::outer::inner::Local::run";
    assert!(
        graph_contains_edge(&graph, dispatch, target),
        "inline-mod sibling import `use inner::Local` from `mod outer` \
         must trace to `{target}`. dispatch callees: {:?}",
        callees_of(&graph, dispatch),
    );
}

// ═══════════════════════════════════════════════════════════════════
// Bug 8 (v1.2.4) — multi-bound generic receiver: later bound wins
// ═══════════════════════════════════════════════════════════════════
//
// `Q: Audit + Handler` where `Audit` has no `handle` method but
// `Handler` does. `q.handle()` must emit edge to `Handler::handle`,
// not silently drop because the FIRST bound (`Audit`) doesn't define
// the method. Codex P2 round 7 (2026-05-14) flagged that
// `generic_param_bound` returned only the first non-empty bound,
// breaking method dispatch when the bound order doesn't match the
// method's defining trait. The UFCS form (`Q::handle()`) already
// emits one edge per bound; the receiver form was asymmetric.

#[test]
fn bug8_multi_bound_receiver_dispatch_finds_method_on_later_bound() {
    let ws = build_workspace(&[
        (
            "src/application/mod.rs",
            r#"
            pub trait Audit {
                fn audit(&self);
            }

            pub trait Handler {
                fn handle(&self);
            }

            pub fn dispatch<Q: Audit + Handler>(q: &Q) {
                q.handle();
            }
            "#,
        ),
        (
            "src/cli/mod.rs",
            r#"
            use crate::application::{dispatch, Audit, Handler};

            pub struct Real;
            impl Audit for Real {
                fn audit(&self) {}
            }
            impl Handler for Real {
                fn handle(&self) {}
            }

            pub fn cmd_run() {
                dispatch(&Real);
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
    let dispatch = "crate::application::dispatch";
    let handler_anchor = "crate::application::Handler::handle";
    assert!(
        graph_contains_edge(&graph, dispatch, handler_anchor),
        "multi-bound generic receiver `q: &Q` where `Q: Audit + Handler`: \
         `q.handle()` must emit edge to `{handler_anchor}` even though \
         `Handler` is the SECOND bound and `Audit` (first) doesn't define \
         the method. dispatch callees: {:?}",
        callees_of(&graph, dispatch),
    );
}
