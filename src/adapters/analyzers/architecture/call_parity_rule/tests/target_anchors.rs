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
    let caps = graph.target_anchor_capabilities("application");
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
    let layers = graph.trait_method_anchors.get(anchor).unwrap_or_else(|| {
        panic!(
            "anchor not registered, got {:?}",
            graph.trait_method_anchors
        )
    });
    assert!(
        layers.contains("application"),
        "cross-file impls in application must resolve to layer `application`, got {layers:?}"
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
    let caps = graph.target_anchor_capabilities("application");
    assert!(
        !caps.contains("crate::ports::cli_only::CliOnly::handle"),
        "anchor with no impl in target layer must NOT appear in target capabilities; got {caps:?}"
    );
}
