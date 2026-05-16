//! Workspace-level tests for module-path resolution as the call graph
//! canonicalises call sites and `use` imports.
//!
//! Each test sets up a multi-file (or inline-mod) workspace, builds
//! the call graph, and asserts whether a given import or call site
//! resolves to the expected canonical path. Coverage:
//!
//! - Sibling-submodule `use foo::X` taking precedence over a
//!   crate-root `foo` with the same leaf name (Rust 2018+ resolution).
//! - Inline `mod foo { mod bar; use bar::X; }` blocks contributing
//!   module-path entries equal to filesystem-derived siblings.
//! - Orphan files (no `mod` declaration backing them) NOT acting as
//!   sibling submodules — Rust ignores them, so we must too.
//! - Private `mod X;` declarations inside a file whose ancestor chain
//!   is hidden by a non-root private `mod` still acting as real
//!   children for code inside the parent file.
//! - `pub use` re-exports forwarding the call site to the real fn
//!   definition rather than dropping the edge at the re-export path.
//! - Concrete UFCS (`ConcreteType::new(...)`) where the type is
//!   named-imported from a sibling submodule — common
//!   middleware-/builder-pattern shape.

use super::support::{
    build_graph_only, build_workspace, callees_of, empty_cfg_test, graph_contains_edge, three_layer,
};
use std::collections::HashSet;

#[test]
fn concrete_ufcs_via_sibling_submodule_import_traces_edge() {
    // `dispatcher<T>` body calls `ConcreteOutput::new(body)` where
    // `ConcreteOutput` is imported via `use response::ConcreteOutput`
    // (relative sibling-submodule). The edge must resolve to the
    // sibling's inherent `new` even though the call lives in a
    // generic-fn body.
    let ws = build_workspace(&[
        (
            "src/application/mod.rs",
            r#"
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
            pub struct ConcreteOutput { pub body: String }
            impl ConcreteOutput {
                pub fn new(body: String) -> Self { Self { body } }
            }
            "#,
        ),
        (
            "src/cli/mod.rs",
            r#"
            use crate::application::dispatcher;

            pub fn cmd_run() -> String {
                let out = dispatcher(&42u32);
                out.body
            }
            "#,
        ),
    ]);
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());

    let dispatcher = "crate::application::dispatcher";
    let new_target = "crate::application::response::ConcreteOutput::new";
    assert!(
        graph_contains_edge(&graph, dispatcher, new_target),
        "concrete UFCS `ConcreteOutput::new(body)` inside generic \
         `dispatcher<T>` body must emit edge to `{new_target}` when \
         `ConcreteOutput` is named-imported from a sibling submodule. \
         dispatcher callees: {:?}",
        callees_of(&graph, dispatcher),
    );
}

#[test]
fn sibling_submodule_wins_over_crate_root_with_same_leaf_name() {
    // Rust 2018+: `use foo::X` inside `crate::application` resolves
    // to `crate::application::foo::X` (local sibling) even when
    // `crate::foo` also exists. The normaliser must check the sibling
    // branch BEFORE crate-root modules.
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
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());
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

#[test]
fn inline_mod_sibling_import_traces_edge() {
    // `mod outer { mod inner { ... } use inner::X; }` — inline `mod`
    // blocks must contribute the same module-path entries as
    // filesystem-derived modules so `use inner::Local` resolves to
    // `crate::application::outer::inner::Local`.
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
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());
    let dispatch = "crate::application::outer::dispatch";
    let target = "crate::application::outer::inner::Local::run";
    assert!(
        graph_contains_edge(&graph, dispatch, target),
        "inline-mod sibling import `use inner::Local` from `mod outer` \
         must trace to `{target}`. dispatch callees: {:?}",
        callees_of(&graph, dispatch),
    );
}

#[test]
fn orphan_file_does_not_act_as_sibling_submodule() {
    // `application/orphan.rs` exists on disk but `application/mod.rs`
    // never declares `mod orphan;` — Rust treats it as dead code.
    // The sibling-submodule discriminator must filter by reachability
    // through `mod` declarations, otherwise `use orphan::X` from a
    // sibling fabricates a false edge to `crate::application::orphan::X`.
    let ws = build_workspace(&[
        (
            "src/application/mod.rs",
            r#"
            use orphan::Local;

            pub fn dispatch() {
                Local::run();
            }
            "#,
        ),
        (
            "src/application/orphan.rs",
            r#"
            pub struct Local;
            impl Local {
                pub fn run() {}
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
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());
    let dispatch = "crate::application::dispatch";
    let phantom_target = "crate::application::orphan::Local::run";
    assert!(
        !graph_contains_edge(&graph, dispatch, phantom_target),
        "orphan file `application/orphan.rs` (no `mod orphan;` decl in \
         application/mod.rs) must NOT be treated as a real sibling \
         submodule — `use orphan::Local` is an external/unresolved \
         import, not a workspace canonical. dispatch callees: {:?}",
        callees_of(&graph, dispatch),
    );
}

#[test]
fn private_mod_sibling_import_resolves_even_when_ancestor_chain_is_hidden() {
    // `src/lib.rs` declares `pub mod application;`. Inside that,
    // `application/mod.rs` declares a private `mod hidden;` — that
    // marks `application/hidden.rs` as not externally reachable per
    // the file-root visibility chain. But from inside `hidden.rs`,
    // `use response::Local;` must still normalise to
    // `crate::application::hidden::response::Local`: `mod response;`
    // is declared in `hidden.rs` itself, so it's a real child module
    // for code inside the parent file, regardless of how the chain
    // is filtered for external visibility.
    let ws = build_workspace(&[
        (
            "src/lib.rs",
            r#"
            pub mod application;
            "#,
        ),
        (
            "src/application/mod.rs",
            r#"
            mod hidden;
            "#,
        ),
        (
            "src/application/hidden.rs",
            r#"
            mod response;

            use response::Local;

            pub fn dispatch() {
                Local::run();
            }
            "#,
        ),
        (
            "src/application/hidden/response.rs",
            r#"
            pub struct Local;
            impl Local {
                pub fn run() {}
            }
            "#,
        ),
    ]);
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());
    let dispatch = "crate::application::hidden::dispatch";
    let target = "crate::application::hidden::response::Local::run";
    assert!(
        graph_contains_edge(&graph, dispatch, target),
        "private `mod response;` declared INSIDE a file whose ancestor \
         chain is hidden by a non-root private `mod hidden;` is still a \
         real child module for code inside `hidden.rs`. \
         `use response::Local` must normalise to `{target}` regardless \
         of public-visibility filtering. dispatch callees: {:?}",
        callees_of(&graph, dispatch),
    );
}

#[test]
fn pub_use_reexport_call_resolves_to_real_definition() {
    // Caller imports `middleware::record_operation` — re-exported via
    // `pub use savings_recorder::record_operation;`. The canonicaliser
    // must follow the `pub use` to the real definition or the edge
    // is dropped.
    let ws = build_workspace(&[
        (
            "src/application/middleware/mod.rs",
            r#"
            pub mod savings_recorder;
            pub use savings_recorder::record_operation;
            "#,
        ),
        (
            "src/application/middleware/savings_recorder.rs",
            r#"
            pub fn record_operation() {}
            "#,
        ),
        (
            "src/application/session.rs",
            r#"
            use crate::application::middleware;
            pub struct Session;
            impl Session {
                pub fn search(&self) {
                    middleware::record_operation();
                }
            }
            "#,
        ),
    ]);
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());

    let search = "crate::application::session::Session::search";
    let real_target = "crate::application::middleware::savings_recorder::record_operation";

    assert!(
        graph_contains_edge(&graph, search, real_target),
        "call site `middleware::record_operation()` not resolved through \
         the `pub use` re-export to the real definition `{real_target}`.\n\
         search callees: {:?}",
        callees_of(&graph, search),
    );
}
