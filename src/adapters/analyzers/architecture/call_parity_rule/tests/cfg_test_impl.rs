//! Tests for cfg-test impl-block scope coverage in graph and pub-fn
//! visitors. Production graph + pub-fn surface must NEVER include
//! methods declared inside `#[cfg(test)] impl X { … }` blocks — the
//! attribute lives on the impl block, not on each child method, so a
//! visitor that only checks per-method attrs lets test-only methods
//! leak into the call graph and pub-fn set, where they could falsely
//! satisfy adapter-coverage or trigger spurious orphan findings.

use super::support::{build_graph_only, build_workspace, empty_cfg_test, three_layer};
use crate::adapters::analyzers::architecture::call_parity_rule::pub_fns::collect_pub_fns_by_layer;
use std::collections::HashSet;

#[test]
fn file_fn_collector_skips_cfg_test_impl_block() {
    // `#[cfg(test)] impl X { pub fn helper(&self) {} }` — the attribute
    // is on the impl, child fns have no cfg-test attr of their own.
    // The graph builder must skip the whole block; otherwise
    // `crate::application::s::X::helper` enters the production graph
    // and could (a) satisfy Check A as a fake target, (b) be reached
    // by an unrelated production caller and produce phantom edges,
    // (c) appear in Check B's pub-fn surface.
    let ws = build_workspace(&[(
        "src/application/s.rs",
        r#"
        pub struct X;
        #[cfg(test)]
        impl X {
            pub fn helper(&self) {}
        }
        "#,
    )]);
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());
    assert!(
        !graph
            .forward
            .contains_key("crate::application::s::X::helper"),
        "cfg-test impl block must not contribute production graph nodes; got {:?}",
        graph.forward.keys().collect::<Vec<_>>()
    );
}

#[test]
fn record_trait_impl_excludes_cfg_test_overrides_from_overridden_set() {
    // Mixed impl block: one production method + one `#[cfg(test)]`
    // method on the same `impl Handler for X { … }`. The trait-impl
    // index must record the production method as overriding, but the
    // test-only method must NOT enter the override set — otherwise
    // production dispatch on `dyn Handler.helper()` would route to
    // a phantom `X::helper` (whose body is cfg-test-gated and never
    // present in production builds).
    //
    // We assert via the anchor capability set: an anchor for `helper`
    // has NO production impl in the workspace, so `target_anchor_capabilities`
    // for the target layer must NOT include it (the overridden set
    // for `helper` is empty after filtering, so no impl-layer is
    // recorded for the helper anchor).
    let ws = build_workspace(&[(
        "src/application/h.rs",
        r#"
        pub trait Handler {
            fn handle(&self);
            fn helper(&self);
        }
        pub struct X;
        impl Handler for X {
            fn handle(&self) {}
            #[cfg(test)]
            fn helper(&self) {}
        }
        "#,
    )]);
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());
    let caps: std::collections::HashSet<&str> = graph
        .target_anchor_capabilities("application", &[])
        .map(|(name, _)| name)
        .collect();
    assert!(
        caps.contains("crate::application::h::Handler::handle"),
        "production method must be present as anchor capability, got {caps:?}"
    );
    assert!(
        !caps.contains("crate::application::h::Handler::helper"),
        "cfg-test method must NOT be in target anchor capabilities — its only override is test-only and `record_trait_impl` filters it out, leaving the override set empty for helper, so no impl-layer is recorded; got {caps:?}"
    );
}

#[test]
fn pub_fns_skips_cfg_test_impl_block() {
    // Sister-fix to file_fn_collector_skips_cfg_test_impl_block — the
    // pub-fn collector has the same impl-block shape and must apply
    // the same guard so test-only impl methods don't enter the
    // target pub-fn set.
    let ws = build_workspace(&[(
        "src/application/s.rs",
        r#"
        pub struct X;
        #[cfg(test)]
        impl X {
            pub fn helper(&self) {}
        }
        "#,
    )]);
    let borrowed: Vec<(&str, &syn::File)> =
        ws.files.iter().map(|(p, _, f)| (p.as_str(), f)).collect();
    let by_layer = collect_pub_fns_by_layer(
        &borrowed,
        &ws.aliases_per_file,
        &three_layer(),
        &empty_cfg_test(),
        &HashSet::new(),
    );
    let app_fn_names: Vec<&str> = by_layer
        .get("application")
        .map(|infos| infos.iter().map(|i| i.fn_name.as_str()).collect())
        .unwrap_or_default();
    assert!(
        !app_fn_names.contains(&"helper"),
        "cfg-test impl method must not enter pub-fn set; got {app_fn_names:?}"
    );
}
