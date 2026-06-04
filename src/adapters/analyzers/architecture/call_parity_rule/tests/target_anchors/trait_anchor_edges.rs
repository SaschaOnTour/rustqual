use super::*;

// ─────────────────────────────────────────────────────────────────────
// Generic-param dispatch (`Q: Trait`) must emit trait-anchor edges
// regardless of bound spelling (inline, where-clause, impl-level) or
// call shape (UFCS path-call vs method-call receiver).
// ─────────────────────────────────────────────────────────────────────

/// Bound-spelling variants for the UFCS/generic-dispatch trait-anchor test:
/// `Q: SymbolQuery` as a where-clause on a free fn, an impl-level bound, and a
/// method-level where-clause on an impl-level generic. A `const` (string-literal
/// fixtures) so it stays `'static` for `graph_3l_of` and never trips LONG_FN.
const UFCS_BOUND_VARIANTS: &[(&str, WsFiles, &str)] = &[
    (
        "where-clause on a free fn",
        &[
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
        ],
        "crate::application::runner::run",
    ),
    (
        // The Q bound lives on the IMPL block, not the method sig.
        "impl-level bound on the impl block",
        &[
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
        ],
        "crate::application::runner::Runner::run",
    ),
    (
        // Bound only in the method's where clause, not on the impl —
        // requires merging the method-where against impl-level names.
        "method-level where clause on an impl-level generic",
        &[
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
        ],
        "crate::application::runner::Runner::run",
    ),
];

#[test]
fn ufcs_path_call_on_generic_param_emits_trait_anchor_edge() {
    // `pub fn run<Q: SymbolQuery>(q: Q) { Q::execute(&q); }` must emit
    // an edge to the trait anchor `SymbolQuery::execute`, the same
    // form that `populate_anchor_index` registers.
    let anchor = "crate::application::symbol::SymbolQuery::execute";

    // Inline-bound free fn `pub fn run<Q: SymbolQuery>(q: Q)`: the edge may
    // land on the anchor OR the concrete impl method (both acceptable here).
    let runner = "crate::application::runner::run";
    let impl_method = "crate::application::symbol::DepsQuery::execute";
    let graph = graph_3l_of(&[
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
    assert!(
        graph_contains_edge(&graph, runner, anchor)
            || graph_contains_edge(&graph, runner, impl_method),
        "`Q::execute(&q)` emits no edge to either the trait anchor `{anchor}` \
         or the impl method `{impl_method}`.\n run callees: {:?}",
        callees_of(&graph, runner),
    );

    // The bound-spelling variants (`UFCS_BOUND_VARIANTS`) must each emit the
    // edge straight to the anchor, regardless of where the `Q: SymbolQuery`
    // bound is written.
    for (label, files, caller) in UFCS_BOUND_VARIANTS {
        let graph = graph_3l_of(files);
        assert!(
            graph_contains_edge(&graph, caller, anchor),
            "case {label}: bound-spelling variant must emit trait-anchor edge \
             {caller} → {anchor}; callees: {:?}",
            callees_of(&graph, caller),
        );
    }
}

/// The `application` source for the method-call-vs-UFCS control/bug pair:
/// `UfcsTrait::execute` (associated fn) + `MethodTrait::execute` (`&self`), each
/// with an impl and a generic `dispatch_*<Q: …Trait>` runner. A `const` (not a
/// fn) so it stays `'static` for `graph_3l_of`'s `WsFiles` and never trips LONG_FN.
const METHOD_CALL_APP_SRC: &str = r#"
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
            "#;

#[test]
fn method_call_on_generic_receiver_emits_trait_anchor_edge() {
    // Side-by-side control + bug pair: UFCS form (`Q::execute(input)`)
    // and method-call form (`q.execute(input)`) go through different
    // resolution paths. Both must emit the trait-anchor edge.
    let graph = graph_3l_of(&[
        ("src/application/mod.rs", METHOD_CALL_APP_SRC),
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

// `q.method()` on a generic-param receiver must emit the trait-anchor edge
// regardless of where the bound is spelled (where-clause, impl-level) or which
// bound in a multi-bound list defines the method. (label, files, caller, anchor)
