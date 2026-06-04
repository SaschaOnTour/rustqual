use super::*;

// Codex P2 sister-site workspace: the graph collector resolves `impl
// crate::application::Hidden { pub fn op() }` through the reexport map, but the
// pub-fn surface collector built its `CanonScope` with `reexports: None` — so the
// graph edge landed on the DECL canonical while pub-fn enumeration used the
// REEXPORT canonical, and Check B/D produced a phantom "not reached" finding. The
// inherent impl lives in a SEPARATE file via an ABSOLUTE crate-path (bypasses the
// file alias map → bare `resolve_to_crate_absolute` → REEXPORT path). After the
// fix, check_b reports zero findings on `Hidden::op`.
const IMPL_SELF_TYPE_WS: &[(&str, &str)] = &[
    (
        "src/lib.rs",
        "pub mod application;\npub mod cli;\npub mod mcp;\n",
    ),
    (
        "src/application/mod.rs",
        r#"
            pub mod private_mod;
            pub mod extras;
            pub use private_mod::Hidden;
            "#,
    ),
    (
        "src/application/private_mod.rs",
        r#"
            pub struct Hidden;
            "#,
    ),
    (
        "src/application/extras.rs",
        r#"
            // Inherent impl on a re-exported type, written via the
            // absolute crate path. This is the bug-trigger shape:
            // without the gate's reexport substitution applied to
            // BOTH the graph collector AND the pub_fns collector,
            // they disagree on the canonical.
            impl crate::application::Hidden {
                pub fn op(&self) -> u32 { 42 }
            }
            "#,
    ),
    (
        "src/cli/mod.rs",
        r#"
            use crate::application::Hidden;
            pub fn handle(h: &Hidden) -> u32 { Hidden::op(h) }
            "#,
    ),
    (
        "src/mcp/mod.rs",
        r#"
            use crate::application::Hidden;
            pub fn handle(h: &Hidden) -> u32 { Hidden::op(h) }
            "#,
    ),
];

#[test]
fn pub_fns_impl_self_type_resolves_through_reexport() {
    let ws = build_workspace(IMPL_SELF_TYPE_WS);
    let cp = cli_mcp_config(5);
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());
    // The bug-triggering finding is on `Hidden::op` specifically:
    // pub_fns enumerates it under the REEXPORT canonical, while the
    // graph edges land on the DECL canonical, so Check B reports it
    // as unreached. After the fix, no findings on Hidden::op at all.
    let bug_findings: Vec<_> = findings
        .iter()
        .filter(|f| format!("{f:?}").contains("Hidden::op"))
        .collect();
    assert!(
        bug_findings.is_empty(),
        "`impl crate::application::Hidden {{ pub fn op() }}` (absolute \
         path on a re-exported type) must produce a pub-fn canonical \
         that matches the graph edge — both go through the same \
         reexport-aware gate. Otherwise Check B reports phantom \
         missing-adapter findings on Hidden::op. bug findings: {bug_findings:?}",
    );
}

#[test]
#[ignore = "Known limitation v1.2.5 — namespace-aware canonicals land in \
            WorkspaceCanonical migration (docs/plan-workspace-canonical-newtype.md). \
            Same-name type+value pub-use collides on both the file-level alias_map \
            (HashMap<String, AliasTarget>, last-write-wins) AND the workspace \
            reexport map (HashMap<String, String>, last-write-wins). Codex P2 \
            reproducer kept here as regression target for the upcoming migration."]
fn value_reexport_does_not_hijack_trait_bound() {
    // Codex P2 reproducer (kept as ignored regression target):
    //
    //     mod handler_trait { pub trait Handler { fn handle(&self) -> u32; } }
    //     mod handler_fn    { pub fn Handler() -> u32 { 1 } }
    //     pub use handler_trait::Handler;  // TYPE re-export
    //     pub use handler_fn::Handler;     // VALUE re-export — same visible name
    //
    // This is valid Rust because type and value namespaces are
    // separate; the compiler resolves `Q: Handler` to the TRAIT
    // because the bound position is a type-context. rustqual loses
    // that context the moment it inserts both entries into a
    // namespace-blind `HashMap<String, _>` — last write wins, so the
    // VALUE entry ends up as the resolved canonical and the trait
    // bound silently dispatches to a function path.
    //
    // The full fix requires namespace-aware canonicals at two
    // layers:
    //   1. file-level `AliasMap`: needs `Vec<AliasTarget>` per name
    //      (or namespace-classified entries) so same-name type+value
    //      `use` items don't overwrite each other.
    //   2. workspace `ReexportMap`: same — split into `type_ns` and
    //      `value_ns` maps with namespace classification at the
    //      `pub use`-leaf collection site.
    //   3. `CanonScope`: gains a `namespace: Namespace` field set
    //      by the caller (Type for trait bounds / impl self-types,
    //      Value for call expressions).
    //
    // Both layers are converging in the planned
    // `WorkspaceCanonical { path, namespace }` newtype: the gate is
    // the sole constructor and produces canonicals that carry their
    // namespace at the type level, so the entire bug class becomes
    // compile-time impossible.
    //
    // See `docs/plan-workspace-canonical-newtype.md` Phase 1 for the
    // structural fix. Unignore this test once that lands.
    let ws = build_workspace(VALUE_REEXPORT_WS);
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());
    let dispatch_canonical = "crate::application::dispatch";
    let trait_method = "crate::application::handler_trait::Handler::handle";
    assert!(
        graph_contains_edge(&graph, dispatch_canonical, trait_method),
        "`Q: Handler` trait-bound dispatch must resolve to the \
         trait's DECL canonical `{trait_method}`. A value-side \
         re-export in the same map must NOT hijack the type-context \
         resolution. dispatch callees: {:?}",
        callees_of(&graph, dispatch_canonical),
    );
}

// Codex P2 reproducer workspace: a TYPE re-export (`pub use handler_trait::Handler`)
// and a VALUE re-export (`pub use handler_fn::Handler`) collide on the same visible
// name in different Rust namespaces; `Q: Handler` (type-context) must still resolve
// to the trait DECL canonical, not the value-side function path.
const VALUE_REEXPORT_WS: &[(&str, &str)] = &[
    ("src/lib.rs", "pub mod application;\npub mod cli;\n"),
    (
        "src/application/mod.rs",
        r#"
            pub mod handler_trait;
            pub mod handler_fn;
            pub mod impl_a;

            // Codex's reproducer: two `pub use ... Handler` items
            // collide on the same string key but live in different
            // Rust namespaces (TYPE + VALUE).
            pub use handler_trait::Handler;
            pub use handler_fn::Handler;

            pub fn dispatch<Q: Handler>(q: &Q) -> u32 { q.handle() }
            "#,
    ),
    (
        "src/application/handler_trait.rs",
        r#"
            pub trait Handler {
                fn handle(&self) -> u32;
            }
            "#,
    ),
    (
        "src/application/handler_fn.rs",
        r#"
            #[allow(non_snake_case)]
            pub fn Handler() -> u32 { 1 }
            "#,
    ),
    (
        "src/application/impl_a.rs",
        r#"
            use crate::application::Handler;
            pub struct QueryA;
            impl Handler for QueryA {
                fn handle(&self) -> u32 { 7 }
            }
            "#,
    ),
    (
        "src/cli/mod.rs",
        r#"
            use crate::application::{dispatch, impl_a::QueryA};
            pub fn run(q: &QueryA) -> u32 { dispatch(q) }
            "#,
    ),
];

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
