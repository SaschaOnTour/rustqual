//! Workspace-level tests for `pub use` re-export resolution.
//!
//! Mirrors the four external repros under `/mnt/d/KI/rustqual-repros/`:
//!
//! 1. `non_pub_helper_resolves` — Repro 01 (non-pub helper canonicalises).
//!    Smoke-test that the v1.2.2 fix still holds.
//! 2. `pub_use_free_fn_resolves` — Repro 02 (`pub use foo::bar;` for a
//!    free fn). Smoke-test that the v1.2.4 fix still holds.
//! 3. `pub_use_trait_dispatch_routes_to_decl` — Repro 03. `pub use
//!    Trait;` + generic dispatch `Q::method()` / `q.method()`. The
//!    dispatch edge must land on the trait's DECL canonical, matching
//!    the anchor index built from `trait_methods` (DECL-keyed) so
//!    `propagate_anchor_impls` fans out to the impls. The 2×2 matrix
//!    (re-exported vs. direct path × `&self` vs. associated fn) is
//!    exercised in one workspace; all four variants must resolve.
//! 4. `pub_use_struct_assoc_fn_routes_to_decl` — Repro 04. `pub use
//!    Struct;` + `Struct::new()`. The associated-fn callee must land
//!    on `crate::decl::Struct::new`, not the re-export path.

use super::support::{
    build_graph_only, build_workspace, callees_of, empty_cfg_test, graph_contains_edge, three_layer,
};
use std::collections::HashSet;

#[test]
fn non_pub_helper_resolves_after_v122_fix() {
    // Smoke-test mirroring repro `01-pass-non-pub-helper`: a non-pub
    // helper in `application/` is called by a pub fn in the same
    // module. The walker must canonicalise both and emit the edge.
    let ws = build_workspace(&[
        (
            "src/lib.rs",
            r#"
            pub mod application;
            pub mod cli;
            "#,
        ),
        (
            "src/application/mod.rs",
            r#"
            pub fn dispatch() -> u32 { helper() }
            fn helper() -> u32 { 42 }
            "#,
        ),
        (
            "src/cli/mod.rs",
            r#"
            use crate::application::dispatch;
            pub fn run() -> u32 { dispatch() }
            "#,
        ),
    ]);
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());
    let dispatch = "crate::application::dispatch";
    let helper = "crate::application::helper";
    assert!(
        graph_contains_edge(&graph, dispatch, helper),
        "non-pub helper edge `{dispatch}` → `{helper}` must be \
         emitted. dispatch callees: {:?}",
        callees_of(&graph, dispatch),
    );
}

#[test]
fn pub_use_free_fn_resolves_after_v124_fix() {
    // Smoke-test mirroring repro `02-pass-pub-use-free-fn`: a free fn
    // re-exported via `pub use` is callable through the re-export path.
    // The post-pass `rewrite_reexport_edges` already handles this since
    // v1.2.4 (exact-match on free-fn callees).
    let ws = build_workspace(&[
        (
            "src/lib.rs",
            r#"
            pub mod application;
            pub mod cli;
            "#,
        ),
        (
            "src/application/mod.rs",
            r#"
            pub mod helpers;
            pub use helpers::record_op;
            "#,
        ),
        (
            "src/application/helpers.rs",
            r#"
            pub fn record_op() -> u32 { 7 }
            "#,
        ),
        (
            "src/cli/mod.rs",
            r#"
            use crate::application::record_op;
            pub fn run() -> u32 { record_op() }
            "#,
        ),
    ]);
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());
    let run = "crate::cli::run";
    let target = "crate::application::helpers::record_op";
    assert!(
        graph_contains_edge(&graph, run, target),
        "`pub use helpers::record_op;` must let `run` → `{target}` \
         resolve via the decl path, not the re-export path. \
         run callees: {:?}",
        callees_of(&graph, run),
    );
}

#[test]
fn pub_use_trait_dispatch_routes_to_decl() {
    // Repro 03 — 2×2 matrix in one workspace:
    // Axis 1: trait reached via `pub use` vs. via decl path.
    // Axis 2: dispatch via `q.run()` (&self) vs. `Q::run()` (UFCS).
    //
    // All four dispatcher fns must produce an edge to the matching
    // impl method, regardless of how the trait is named at the impl
    // site or the dispatcher's bound. Today (pre-fix), variants A+B
    // (re-export side) fail because dispatch edges land on
    // `<reexport>::run` while anchor index is keyed on
    // `<decl>::run` — no fan-out.
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
            pub mod reexport_self;
            pub mod reexport_assoc;
            pub mod direct_self;
            pub mod direct_assoc;
            pub mod impls;
            pub mod dispatchers;

            // Re-export the two "reexport-side" traits — this is the
            // bug-triggering shape.
            pub use reexport_self::ReexportSelf;
            pub use reexport_assoc::ReexportAssoc;
            "#,
        ),
        (
            "src/application/reexport_self.rs",
            r#"
            pub trait ReexportSelf {
                fn run(&self) -> u32;
            }
            "#,
        ),
        (
            "src/application/reexport_assoc.rs",
            r#"
            pub trait ReexportAssoc {
                fn run() -> u32;
            }
            "#,
        ),
        (
            "src/application/direct_self.rs",
            r#"
            pub trait DirectSelf {
                fn run(&self) -> u32;
            }
            "#,
        ),
        (
            "src/application/direct_assoc.rs",
            r#"
            pub trait DirectAssoc {
                fn run() -> u32;
            }
            "#,
        ),
        (
            "src/application/impls.rs",
            r#"
            // Impl side: reach the trait through the re-export OR
            // the decl path, matching each test variant.
            use crate::application::direct_assoc::DirectAssoc;
            use crate::application::direct_self::DirectSelf;
            use crate::application::ReexportAssoc;
            use crate::application::ReexportSelf;

            pub struct QueryA;
            pub struct QueryB;
            pub struct QueryC;
            pub struct QueryD;

            impl ReexportSelf for QueryA {
                fn run(&self) -> u32 { 1 }
            }
            impl ReexportAssoc for QueryB {
                fn run() -> u32 { 2 }
            }
            impl DirectSelf for QueryC {
                fn run(&self) -> u32 { 3 }
            }
            impl DirectAssoc for QueryD {
                fn run() -> u32 { 4 }
            }
            "#,
        ),
        (
            "src/application/dispatchers.rs",
            r#"
            use crate::application::direct_assoc::DirectAssoc;
            use crate::application::direct_self::DirectSelf;
            use crate::application::ReexportAssoc;
            use crate::application::ReexportSelf;

            pub fn dispatch_a<Q: ReexportSelf>(q: &Q) -> u32 { q.run() }
            pub fn dispatch_b<Q: ReexportAssoc>() -> u32 { Q::run() }
            pub fn dispatch_c<Q: DirectSelf>(q: &Q) -> u32 { q.run() }
            pub fn dispatch_d<Q: DirectAssoc>() -> u32 { Q::run() }
            "#,
        ),
    ]);
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());

    // All four dispatchers must dispatch to their trait's DECL canonical
    // (the anchor key). After fan-out the concrete impl method becomes
    // reachable, but at the dispatch-edge level we assert against the
    // trait-method anchor canonical.
    let checks = [
        (
            "dispatch_a (re-export, &self)",
            "crate::application::dispatchers::dispatch_a",
            "crate::application::reexport_self::ReexportSelf::run",
        ),
        (
            "dispatch_b (re-export, assoc fn)",
            "crate::application::dispatchers::dispatch_b",
            "crate::application::reexport_assoc::ReexportAssoc::run",
        ),
        (
            "dispatch_c (decl path, &self)",
            "crate::application::dispatchers::dispatch_c",
            "crate::application::direct_self::DirectSelf::run",
        ),
        (
            "dispatch_d (decl path, assoc fn)",
            "crate::application::dispatchers::dispatch_d",
            "crate::application::direct_assoc::DirectAssoc::run",
        ),
    ];
    for (label, caller, target) in &checks {
        assert!(
            graph_contains_edge(&graph, caller, target),
            "{label}: `{caller}` must emit a dispatch edge to the \
             trait's DECL canonical `{target}`. Today the re-export-side \
             variants land on `crate::application::ReexportSelf::run` \
             (re-export path), which doesn't match the anchor index. \
             caller callees: {:?}",
            callees_of(&graph, caller),
        );
    }
}

#[test]
fn pub_use_struct_assoc_fn_routes_to_decl() {
    // Repro 04: `pub use Struct;` + `Struct::new()`. The associated-fn
    // callee must land on the struct's DECL canonical, matching the
    // inherent-impl entry registered in the type index. Today the
    // callee carries the re-export path and the inherent-impl lookup
    // misses — `Struct::new` is reported as unreached.
    let ws = build_workspace(&[
        (
            "src/lib.rs",
            r#"
            pub mod application;
            pub mod cli;
            "#,
        ),
        (
            "src/application/mod.rs",
            r#"
            pub mod response;
            pub mod consumer;
            pub use response::OperationResponse;
            "#,
        ),
        (
            "src/application/response.rs",
            r#"
            pub struct OperationResponse { pub body: String }
            impl OperationResponse {
                pub fn new(body: String) -> Self { Self { body } }
            }
            "#,
        ),
        (
            "src/application/consumer.rs",
            r#"
            use super::OperationResponse;
            pub fn record_op() -> OperationResponse {
                OperationResponse::new("ok".to_string())
            }
            "#,
        ),
        (
            "src/cli/mod.rs",
            r#"
            use crate::application::consumer::record_op;
            pub fn run() -> String { record_op().body }
            "#,
        ),
    ]);
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());
    let record_op = "crate::application::consumer::record_op";
    let target = "crate::application::response::OperationResponse::new";
    assert!(
        graph_contains_edge(&graph, record_op, target),
        "`OperationResponse::new()` called via the `pub use`-imported \
         alias must resolve to the DECL canonical `{target}`, not the \
         re-export-rooted path. record_op callees: {:?}",
        callees_of(&graph, record_op),
    );
}
