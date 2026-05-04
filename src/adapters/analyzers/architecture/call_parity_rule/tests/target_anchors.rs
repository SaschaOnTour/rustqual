//! Tests for the target-anchor capability surface — the set of
//! synthetic `<Trait>::<method>` anchors that count as target-layer
//! capabilities for Check B/D coverage decisions. Anchors are
//! capabilities (not concrete fns), and adapter coverage of a
//! `dyn Trait.method()` dispatch must be checked against them, not
//! against the concrete impls those dispatches reach.

use super::support::{build_graph_only, build_workspace, empty_cfg_test, ports_app_cli_mcp};
use std::collections::HashSet;

#[test]
fn target_capabilities_include_trait_anchor_when_overriding_impl_in_target_layer() {
    // Hexagonal layout: trait in `ports`, overriding impl in
    // `application` (target). Adapter dispatch via `dyn Handler`
    // reaches the anchor — so the anchor MUST appear in the target
    // capability set, otherwise Check B/D have no way to enumerate
    // it as a target-side capability.
    let ws = build_workspace(&[
        (
            "src/ports/handler.rs",
            "pub trait Handler { fn handle(&self); }",
        ),
        (
            "src/application/logging.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct LoggingHandler;
            impl Handler for LoggingHandler { fn handle(&self) {} }
            "#,
        ),
    ]);
    let graph = build_graph_only(
        &ws,
        &ports_app_cli_mcp(),
        &empty_cfg_test(),
        &HashSet::new(),
    );
    let caps: std::collections::HashSet<&str> = graph
        .target_anchor_capabilities("application", &[])
        .map(|(name, _)| name)
        .collect();
    assert!(
        caps.contains("crate::ports::handler::Handler::handle"),
        "anchor for trait with overriding impl in target layer must appear in target capabilities; got {caps:?}"
    );
}

#[test]
fn populate_anchor_index_resolves_impl_layers_for_cross_file_impls() {
    // Trait declared in `ports/handler.rs`, two impls in different
    // application files. The anchor's resolved layer set MUST include
    // `application` even though the trait and impls live in distinct
    // files — `populate_anchor_index` resolves each impl canonical
    // through `LayerDefinitions::layer_of_crate_path`, not via the
    // graph's per-node layer cache (which is built from edges, not
    // bare struct types).
    let ws = build_workspace(&[
        (
            "src/ports/handler.rs",
            "pub trait Handler { fn handle(&self); }",
        ),
        (
            "src/application/a.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct A;
            impl Handler for A { fn handle(&self) {} }
            "#,
        ),
        (
            "src/application/b.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct B;
            impl Handler for B { fn handle(&self) {} }
            "#,
        ),
    ]);
    let graph = build_graph_only(
        &ws,
        &ports_app_cli_mcp(),
        &empty_cfg_test(),
        &HashSet::new(),
    );
    let anchor = "crate::ports::handler::Handler::handle";
    let info = graph.trait_method_anchors.get(anchor).unwrap_or_else(|| {
        panic!(
            "anchor not registered, got {:?}",
            graph.trait_method_anchors
        )
    });
    assert!(
        info.impl_layers.contains("application"),
        "cross-file impls in application must resolve to layer `application`, got {:?}",
        info.impl_layers
    );
}

#[test]
fn target_capabilities_exclude_trait_anchor_without_target_layer_impl() {
    // Trait declared in ports, impl in `cli` (an adapter layer, NOT
    // target). The anchor reaches NO target capability — so it must
    // NOT appear in the target capability set; otherwise Check B
    // would falsely demand adapter coverage for a trait that isn't
    // a target-side capability.
    let ws = build_workspace(&[
        (
            "src/ports/cli_only.rs",
            "pub trait CliOnly { fn handle(&self); }",
        ),
        (
            "src/cli/impls.rs",
            r#"
            use crate::ports::cli_only::CliOnly;
            pub struct CliImpl;
            impl CliOnly for CliImpl { fn handle(&self) {} }
            "#,
        ),
    ]);
    let graph = build_graph_only(
        &ws,
        &ports_app_cli_mcp(),
        &empty_cfg_test(),
        &HashSet::new(),
    );
    let caps: std::collections::HashSet<&str> = graph
        .target_anchor_capabilities("application", &[])
        .map(|(name, _)| name)
        .collect();
    assert!(
        !caps.contains("crate::ports::cli_only::CliOnly::handle"),
        "anchor with no impl in target layer must NOT appear in target capabilities; got {caps:?}"
    );
}

#[test]
fn target_anchor_capabilities_rejects_peer_adapter_declared_anchor() {
    // Trait declared INSIDE a peer-adapter layer (`mcp`), with an
    // overriding impl in `application` (target). Without the
    // peer-adapter filter, a `cli` walk could enumerate this anchor
    // as a target capability and fire false missing-adapter findings
    // for an MCP-internal contract that has no business in CLI's
    // surface.
    let ws = build_workspace(&[
        (
            "src/mcp/handler.rs",
            "pub trait Handler { fn handle(&self); }",
        ),
        (
            "src/application/logging.rs",
            r#"
            use crate::mcp::handler::Handler;
            pub struct LoggingHandler;
            impl Handler for LoggingHandler { fn handle(&self) {} }
            "#,
        ),
    ]);
    let graph = build_graph_only(
        &ws,
        &ports_app_cli_mcp(),
        &empty_cfg_test(),
        &HashSet::new(),
    );
    let adapters = ["cli".to_string(), "mcp".to_string()];
    let caps: std::collections::HashSet<&str> = graph
        .target_anchor_capabilities("application", &adapters)
        .map(|(name, _)| name)
        .collect();
    assert!(
        !caps.contains("crate::mcp::handler::Handler::handle"),
        "peer-adapter-declared anchor must NOT appear in target capabilities; got {caps:?}"
    );
}

#[test]
fn target_anchor_capabilities_includes_default_only_target_layer_trait() {
    // Trait declared in the target layer with a default body, no
    // overriding impls anywhere. The default body IS the capability —
    // the anchor must be enumerated as target capability so Check B/D
    // require adapter coverage.
    let ws = build_workspace(&[(
        "src/application/handler.rs",
        // Default body in the trait itself; no impls.
        "pub trait Handler { fn handle(&self) {} }",
    )]);
    let graph = build_graph_only(
        &ws,
        &ports_app_cli_mcp(),
        &empty_cfg_test(),
        &HashSet::new(),
    );
    let caps: std::collections::HashSet<&str> = graph
        .target_anchor_capabilities("application", &[])
        .map(|(name, _)| name)
        .collect();
    assert!(
        caps.contains("crate::application::handler::Handler::handle"),
        "default-only trait method in target layer must be a target capability; got {caps:?}"
    );
}

#[test]
fn target_anchor_capabilities_excludes_private_target_layer_trait() {
    // Codex round 3 P1 (2026-05-04): a private (non-`pub`) trait
    // declared in the target layer is not part of the public
    // architecture surface. Even with a default body it cannot be
    // dispatched to from outside its declaring module — so it must
    // NOT be enumerated as a target capability, otherwise Check B
    // would falsely demand adapter coverage for an implementation
    // detail.
    let ws = build_workspace(&[(
        "src/application/internal.rs",
        // `trait`, not `pub trait` — private.
        "trait Internal { fn run(&self) {} }",
    )]);
    let graph = build_graph_only(
        &ws,
        &ports_app_cli_mcp(),
        &empty_cfg_test(),
        &HashSet::new(),
    );
    let caps: std::collections::HashSet<&str> = graph
        .target_anchor_capabilities("application", &[])
        .map(|(name, _)| name)
        .collect();
    assert!(
        !caps.contains("crate::application::internal::Internal::run"),
        "private target-layer trait must NOT be a target capability anchor; got {caps:?}"
    );
}

#[test]
fn target_anchor_capabilities_excludes_private_ports_trait_with_target_impl() {
    // Codex round 3 P1 (2026-05-04): a private trait declared in
    // ports with an impl in the target layer is not architecturally
    // exposed — the trait isn't usable from outside ports' module.
    // It must NOT be promoted to a target capability via the
    // overriding-impl branch of the unified rule.
    let ws = build_workspace(&[
        (
            "src/ports/internal.rs",
            // `trait`, not `pub trait` — private.
            "trait Hidden { fn run(&self); }",
        ),
        (
            "src/application/impls.rs",
            r#"
            use crate::ports::internal::Hidden;
            pub struct Impl;
            impl Hidden for Impl { fn run(&self) {} }
            "#,
        ),
    ]);
    let graph = build_graph_only(
        &ws,
        &ports_app_cli_mcp(),
        &empty_cfg_test(),
        &HashSet::new(),
    );
    let caps: std::collections::HashSet<&str> = graph
        .target_anchor_capabilities("application", &[])
        .map(|(name, _)| name)
        .collect();
    assert!(
        !caps.contains("crate::ports::internal::Hidden::run"),
        "private ports trait (even with target-layer impl) must NOT be a target capability anchor; got {caps:?}"
    );
}

#[test]
fn target_anchor_capabilities_excludes_pub_trait_inside_private_mod() {
    // Round-3 follow-up (A14 visibility-pre-pass gap class):
    // `pub trait T { ... }` inside a private `mod inner { … }` is
    // syntactically `Public`, but workspace-invisible — the enclosing
    // mod has no `pub`, so external code can't reach the trait via
    // `use crate::application::wrapper::inner::T;`. The simple
    // `matches!(node.vis, Public(_))` check accepts this trait as
    // visible, which lets a private-mod trait surface as a Check B/D
    // capability anchor. The trait-collector must track enclosing-mod
    // visibility (mirroring `pub_fns::PubFnCollector::enclosing_mod_visible`)
    // so a pub trait inside any private ancestor mod is rejected.
    let ws = build_workspace(&[(
        "src/application/wrapper.rs",
        r#"
        mod inner {
            pub trait T { fn run(&self) {} }
        }
        "#,
    )]);
    let graph = build_graph_only(
        &ws,
        &ports_app_cli_mcp(),
        &empty_cfg_test(),
        &HashSet::new(),
    );
    let caps: std::collections::HashSet<&str> = graph
        .target_anchor_capabilities("application", &[])
        .map(|(name, _)| name)
        .collect();
    assert!(
        !caps.contains("crate::application::wrapper::inner::T::run"),
        "pub trait inside a private mod must NOT be a target capability anchor (workspace-invisible); got {caps:?}"
    );
}

#[test]
fn target_anchor_capabilities_excludes_signature_only_target_layer_trait() {
    // Trait declared in target layer but with NO default body and NO
    // overriding impls. Pure signature is uncallable — must NOT count
    // as a capability (regression guard for the cfg-test scenario
    // that tripped during F1 design).
    let ws = build_workspace(&[(
        "src/application/handler.rs",
        // Signature only — no default body.
        "pub trait Handler { fn handle(&self); }",
    )]);
    let graph = build_graph_only(
        &ws,
        &ports_app_cli_mcp(),
        &empty_cfg_test(),
        &HashSet::new(),
    );
    let caps: std::collections::HashSet<&str> = graph
        .target_anchor_capabilities("application", &[])
        .map(|(name, _)| name)
        .collect();
    assert!(
        !caps.contains("crate::application::handler::Handler::handle"),
        "pure-signature trait method (no default, no impl) must NOT be a target capability; got {caps:?}"
    );
}
