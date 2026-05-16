//! Tests for the target-anchor capability surface — the set of
//! synthetic `<Trait>::<method>` anchors that count as target-layer
//! capabilities for Check B/D coverage decisions. Anchors are
//! capabilities (not concrete fns), and adapter coverage of a
//! `dyn Trait.method()` dispatch must be checked against them, not
//! against the concrete impls those dispatches reach.

use super::support::{
    build_graph_only, build_workspace, callees_of, empty_cfg_test, graph_contains_edge,
    ports_app_cli_mcp, three_layer,
};
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
    // A private (non-`pub`) trait isn't workspace-visible and must
    // never count as architectural surface, even with a default body.
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
    // Private ports trait isn't usable outside its module; even a
    // target-impl shouldn't promote it to a target capability.
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
fn target_anchor_capabilities_excludes_inherited_default_when_body_lives_outside_target() {
    // Empty `impl Handler for AppHandler {}` in target inherits the
    // default body, but that body lives on the ports trait — the
    // graph doesn't model it as a target-layer node. The anchor must
    // NOT count as a target capability; otherwise B/D would require
    // adapter coverage for a body that never crosses into target.
    let ws = build_workspace(&[
        (
            "src/ports/handler.rs",
            "pub trait Handler { fn handle(&self) {} }",
        ),
        (
            "src/application/logging.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct AppHandler;
            impl Handler for AppHandler {}
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
        !caps.contains("crate::ports::handler::Handler::handle"),
        "ports default body + empty target impl must NOT promote the anchor to a target capability; got {caps:?}"
    );
}

#[test]
fn target_anchor_capabilities_includes_pub_crate_trait_with_target_impl() {
    // `pub(crate)` traits are workspace-visible per the shared
    // `visible_canonicals` set; the anchor must be enumerated.
    let ws = build_workspace(&[
        (
            "src/ports/handler.rs",
            "pub(crate) trait Handler { fn handle(&self); }",
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
        "pub(crate) trait with target-layer impl must be a target capability anchor (workspace-visible per `is_visible` model); got {caps:?}"
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

// ─────────────────────────────────────────────────────────────────────
// Generic-param dispatch (`Q: Trait`) must emit trait-anchor edges
// regardless of bound spelling (inline, where-clause, impl-level) or
// call shape (UFCS path-call vs method-call receiver).
// ─────────────────────────────────────────────────────────────────────

#[test]
fn ufcs_path_call_on_generic_param_emits_trait_anchor_edge() {
    // `pub fn run<Q: SymbolQuery>(q: Q) { Q::execute(&q); }` must emit
    // an edge to the trait anchor `SymbolQuery::execute`, the same
    // form that `populate_anchor_index` registers.
    let ws = build_workspace(&[
        (
            "src/application/symbol.rs",
            r#"
            pub trait SymbolQuery {
                fn execute(&self);
            }
            pub struct DepsQuery;
            impl SymbolQuery for DepsQuery {
                fn execute(&self) {}
            }
            "#,
        ),
        (
            "src/application/runner.rs",
            r#"
            use crate::application::symbol::SymbolQuery;

            pub fn run<Q: SymbolQuery>(q: Q) {
                Q::execute(&q);
            }
            "#,
        ),
    ]);
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());

    let runner = "crate::application::runner::run";
    let anchor = "crate::application::symbol::SymbolQuery::execute";
    let impl_method = "crate::application::symbol::DepsQuery::execute";

    let has_anchor = graph_contains_edge(&graph, runner, anchor);
    let has_impl = graph_contains_edge(&graph, runner, impl_method);

    // Sanity: where-clause spelling must produce the same edge — both
    // forms are semantically identical.
    let ws_where = build_workspace(&[
        (
            "src/application/symbol.rs",
            r#"
            pub trait SymbolQuery { fn execute(&self); }
            pub struct DepsQuery;
            impl SymbolQuery for DepsQuery { fn execute(&self) {} }
            "#,
        ),
        (
            "src/application/runner.rs",
            r#"
            use crate::application::symbol::SymbolQuery;
            pub fn run<Q>(q: Q) where Q: SymbolQuery { Q::execute(&q); }
            "#,
        ),
    ]);
    let graph_where = build_graph_only(
        &ws_where,
        &three_layer(),
        &empty_cfg_test(),
        &HashSet::new(),
    );
    assert!(
        graph_contains_edge(&graph_where, runner, anchor),
        "where-clause variant must emit the same trait-anchor edge as inline-bound. \
         run callees: {:?}",
        callees_of(&graph_where, runner),
    );

    // Impl-level bound: `impl<Q: SymbolQuery> Runner<Q> { fn run(...) }`.
    // The Q bound lives on the IMPL block, not the method sig.
    let ws_impl = build_workspace(&[
        (
            "src/application/symbol.rs",
            r#"
            pub trait SymbolQuery { fn execute(&self); }
            pub struct DepsQuery;
            impl SymbolQuery for DepsQuery { fn execute(&self) {} }
            "#,
        ),
        (
            "src/application/runner.rs",
            r#"
            use crate::application::symbol::SymbolQuery;
            pub struct Runner<Q>(pub Q);
            impl<Q: SymbolQuery> Runner<Q> {
                pub fn run(&self, q: &Q) { Q::execute(q); }
            }
            "#,
        ),
    ]);
    let graph_impl = build_graph_only(&ws_impl, &three_layer(), &empty_cfg_test(), &HashSet::new());
    let runner_impl = "crate::application::runner::Runner::run";
    assert!(
        graph_contains_edge(&graph_impl, runner_impl, anchor),
        "impl-level generic bound must emit trait-anchor edge for Q::method() inside method body. \
         Runner::run callees: {:?}",
        callees_of(&graph_impl, runner_impl),
    );

    assert!(
        has_anchor || has_impl,
        "`Q::execute(&q)` emits no edge to either the trait anchor \
         `{anchor}` or the impl method `{impl_method}`.\n\
         run callees: {:?}",
        callees_of(&graph, runner),
    );

    // Method-level where clause on impl-level generic: bound only in
    // the method's where clause, not on the impl. Without merging the
    // method-where against impl-level names, the predicate would drop.
    let ws_method_where = build_workspace(&[
        (
            "src/application/symbol.rs",
            r#"
            pub trait SymbolQuery { fn execute(&self); }
            pub struct DepsQuery;
            impl SymbolQuery for DepsQuery { fn execute(&self) {} }
            "#,
        ),
        (
            "src/application/runner.rs",
            r#"
            use crate::application::symbol::SymbolQuery;
            pub struct Runner<Q>(pub Q);
            impl<Q> Runner<Q> {
                pub fn run(&self, q: &Q) where Q: SymbolQuery { Q::execute(q); }
            }
            "#,
        ),
    ]);
    let graph_method_where = build_graph_only(
        &ws_method_where,
        &three_layer(),
        &empty_cfg_test(),
        &HashSet::new(),
    );
    assert!(
        graph_contains_edge(&graph_method_where, runner_impl, anchor),
        "method-level where clause on impl-level generic must emit \
         trait-anchor edge. Runner::run callees: {:?}",
        callees_of(&graph_method_where, runner_impl),
    );
}

#[test]
fn method_call_on_generic_receiver_emits_trait_anchor_edge() {
    // Side-by-side control + bug pair: UFCS form (`Q::execute(input)`)
    // and method-call form (`q.execute(input)`) go through different
    // resolution paths. Both must emit the trait-anchor edge.
    let ws = build_workspace(&[
        (
            "src/application/mod.rs",
            r#"
            use serde::Serialize;

            // Control: trait with associated function (no &self).
            pub trait UfcsTrait {
                type Output: Serialize;
                fn execute(input: &str) -> Self::Output;
            }

            pub fn ufcs_inner(input: &str) -> String {
                format!("ufcs:{input}")
            }

            pub struct UfcsTraitImpl;
            impl UfcsTrait for UfcsTraitImpl {
                type Output = String;
                fn execute(input: &str) -> Self::Output {
                    ufcs_inner(input)
                }
            }

            pub fn dispatch_ufcs<Q: UfcsTrait>(input: &str) -> Q::Output {
                Q::execute(input)
            }

            // Bug shape: trait with `&self` receiver.
            pub trait MethodTrait {
                type Output: Serialize;
                fn execute(&self, input: &str) -> Self::Output;
            }

            pub fn method_inner(input: &str) -> String {
                format!("method:{input}")
            }

            pub struct MethodTraitImpl;
            impl MethodTrait for MethodTraitImpl {
                type Output = String;
                fn execute(&self, input: &str) -> Self::Output {
                    method_inner(input)
                }
            }

            pub fn dispatch_method<Q: MethodTrait>(query: &Q, input: &str) -> Q::Output {
                query.execute(input)
            }
            "#,
        ),
        (
            "src/cli/mod.rs",
            r#"
            use crate::application::{
                dispatch_method, dispatch_ufcs, MethodTraitImpl, UfcsTraitImpl,
            };

            pub fn cmd_ufcs() -> String {
                dispatch_ufcs::<UfcsTraitImpl>("hi")
            }

            pub fn cmd_method() -> String {
                dispatch_method(&MethodTraitImpl, "hi")
            }
            "#,
        ),
    ]);
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());

    let dispatch_method = "crate::application::dispatch_method";
    let method_anchor = "crate::application::MethodTrait::execute";

    // Control: UFCS form must already trace.
    let dispatch_ufcs = "crate::application::dispatch_ufcs";
    let ufcs_anchor = "crate::application::UfcsTrait::execute";
    assert!(
        graph_contains_edge(&graph, dispatch_ufcs, ufcs_anchor),
        "control: UFCS form must already trace. \
         dispatch_ufcs callees: {:?}",
        callees_of(&graph, dispatch_ufcs),
    );

    // The actual coverage: method-call form on generic-param receiver
    // must also emit the trait-anchor edge.
    assert!(
        graph_contains_edge(&graph, dispatch_method, method_anchor),
        "method-call on generic receiver `query.execute(input)` where \
         `query: &Q` and `Q: MethodTrait` must emit trait-anchor edge \
         `{method_anchor}`. dispatch_method callees: {:?}",
        callees_of(&graph, dispatch_method),
    );
}

#[test]
fn method_call_on_where_clause_bound_generic_emits_trait_anchor_edge() {
    // `where Q: T` instead of inline `<Q: T>`. Same edge expected —
    // bound spelling shouldn't affect dispatch resolution.
    let ws = build_workspace(&[(
        "src/application/mod.rs",
        r#"
        pub trait MethodTrait {
            fn execute(&self, input: &str) -> String;
        }

        pub fn dispatch_method<Q>(query: &Q, input: &str) -> String
        where
            Q: MethodTrait,
        {
            query.execute(input)
        }
        "#,
    )]);
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());
    let dispatch_method = "crate::application::dispatch_method";
    let anchor = "crate::application::MethodTrait::execute";
    assert!(
        graph_contains_edge(&graph, dispatch_method, anchor),
        "where-clause bound on generic-param receiver must emit \
         trait-anchor edge. dispatch_method callees: {:?}",
        callees_of(&graph, dispatch_method),
    );
}

#[test]
fn method_call_on_impl_level_generic_emits_trait_anchor_edge() {
    // Bound lives on the impl block, method-call inside the method
    // body: `impl<Q: T> Foo<Q> { fn run(&self, q: &Q) { q.execute(...) } }`.
    let ws = build_workspace(&[(
        "src/application/mod.rs",
        r#"
        pub trait MethodTrait {
            fn execute(&self, input: &str) -> String;
        }

        pub struct Runner<Q>(pub Q);

        impl<Q: MethodTrait> Runner<Q> {
            pub fn run(&self, q: &Q, input: &str) -> String {
                q.execute(input)
            }
        }
        "#,
    )]);
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());
    let runner_run = "crate::application::Runner::run";
    let anchor = "crate::application::MethodTrait::execute";
    assert!(
        graph_contains_edge(&graph, runner_run, anchor),
        "impl-level generic bound on method-call receiver must emit \
         trait-anchor edge. Runner::run callees: {:?}",
        callees_of(&graph, runner_run),
    );
}

#[test]
fn multi_bound_generic_receiver_method_call_emits_anchor_for_defining_bound() {
    // `Q: Audit + Handler` where `Audit` has no `handle` method but
    // `Handler` does. `q.handle()` must emit edge to `Handler::handle`
    // even though `Handler` is the SECOND bound and `Audit` (first)
    // doesn't define the method. UFCS form (`Q::handle()`) already
    // emits one edge per bound; the receiver form must be symmetric.
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
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());
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
