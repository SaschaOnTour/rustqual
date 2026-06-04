//! Tests for cfg-test impl-block scope coverage in graph and pub-fn
//! visitors. Production graph + pub-fn surface must NEVER include
//! methods declared inside `#[cfg(test)] impl X { … }` blocks — the
//! attribute lives on the impl block, not on each child method, so a
//! visitor that only checks per-method attrs lets test-only methods
//! leak into the call graph and pub-fn set, where they could falsely
//! satisfy adapter-coverage or trigger spurious orphan findings.

use super::support::{build_graph_only, build_workspace, empty_cfg_test, three_layer};
use crate::adapters::analyzers::architecture::call_parity_rule::pub_fns::{
    collect_pub_fns_by_layer, PubFnInputs,
};
use std::collections::HashSet;

/// `(label, files, present_anchor, absent_anchor)` for the cfg-test anchor cases.
type AnchorCase = (
    &'static str,
    &'static [(&'static str, &'static str)],
    &'static str,
    &'static str,
);

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

/// Target-layer (`application`) anchor capability names for a workspace.
fn target_anchor_caps(files: &[(&str, &str)]) -> HashSet<String> {
    let ws = build_workspace(files);
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());
    graph
        .target_anchor_capabilities("application", &[])
        .map(|(name, _)| name.to_string())
        .collect()
}

#[test]
fn cfg_test_trait_methods_excluded_from_anchor_capabilities() {
    // A `#[cfg(test)]` trait method — whether it's a test-only impl override
    // (its production override set ends up empty) or a test-only default body
    // on the trait — must NOT become a target anchor capability, while the
    // sibling production method still does. (label, files, present, absent)
    let cases: &[AnchorCase] = &[
        (
            "cfg-test impl override is filtered from the override set",
            &[(
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
            )],
            "crate::application::h::Handler::handle",
            "crate::application::h::Handler::helper",
        ),
        (
            "cfg-test trait default-body method is filtered from trait_methods",
            &[(
                "src/application/h.rs",
                r#"
                pub trait Handler {
                    fn handle(&self) {}
                    #[cfg(test)]
                    fn test_helper(&self) {}
                }
                "#,
            )],
            "crate::application::h::Handler::handle",
            "crate::application::h::Handler::test_helper",
        ),
    ];
    for (label, files, present, absent) in cases {
        let caps = target_anchor_caps(files);
        assert!(
            caps.contains(*present),
            "case {label}: production method `{present}` must be a capability; got {caps:?}"
        );
        assert!(
            !caps.contains(*absent),
            "case {label}: cfg-test method `{absent}` must NOT be a capability; got {caps:?}"
        );
    }
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
    let by_layer = collect_pub_fns_by_layer(PubFnInputs {
        files: &borrowed,
        aliases_per_file: &ws.aliases_per_file,
        layers: &three_layer(),
        transparent_wrappers: &HashSet::new(),
        promoted_attributes: &HashSet::new(),
        workspace: &crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::WorkspaceLookup {
            cfg_test_files: &empty_cfg_test(),
            crate_root_modules: &HashSet::new(),
            workspace_module_paths: &HashSet::new(),
        },
    });
    let app_fn_names: Vec<&str> = by_layer
        .get("application")
        .map(|infos| infos.iter().map(|i| i.fn_name.as_str()).collect())
        .unwrap_or_default();
    assert!(
        !app_fn_names.contains(&"helper"),
        "cfg-test impl method must not enter pub-fn set; got {app_fn_names:?}"
    );
}
