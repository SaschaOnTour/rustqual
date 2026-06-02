use super::*;

#[test]
fn trait_dispatch_emits_trait_method_anchor() {
    // `dyn Handler.handle()` records ONE edge: the synthetic trait-method
    // anchor `<Trait>::<method>`. Concrete impls (`LoggingHandler::handle`,
    // …) are NOT emitted as separate edges from the dispatch site —
    // fanout would create N-way touchpoint sets that fire Check C
    // false-positives for a single boundary call. The anchor represents
    // the logical capability; impl-level reachability is wired in a
    // separate graph pass via `<Trait>::<method> → <Impl>::<method>`.
    let fx = parse(
        r#"
        use crate::ports::Handler;
        pub fn dispatch(h: &dyn Handler) {
            h.handle();
        }
        "#,
    );
    let mut index = WorkspaceTypeIndex::new();
    index.trait_methods.insert(
        "crate::ports::Handler".to_string(),
        std::iter::once("handle".to_string()).collect(),
    );
    index.trait_impls.insert(
        "crate::ports::Handler".to_string(),
        vec![
            "crate::app::LoggingHandler".to_string(),
            "crate::app::MetricsHandler".to_string(),
            "crate::app::AuditHandler".to_string(),
        ],
    );
    let calls = run(&fx, &index, "dispatch");
    assert!(
        calls.contains("crate::ports::Handler::handle"),
        "expected single trait-method anchor edge, got {calls:?}"
    );
    assert!(
        !calls.contains("crate::app::LoggingHandler::handle"),
        "must not emit per-impl fanout edges from dispatch (Check C false-positive source), got {calls:?}"
    );
    assert!(!calls.contains("crate::app::MetricsHandler::handle"));
    assert!(!calls.contains("crate::app::AuditHandler::handle"));
}

/// A `WorkspaceTypeIndex` with the trait `crate::ports::Handler` (one method
/// `handle`) implemented by `impl_path`.
fn handler_index(impl_path: &str) -> WorkspaceTypeIndex {
    let mut index = WorkspaceTypeIndex::new();
    index.trait_methods.insert(
        "crate::ports::Handler".to_string(),
        std::iter::once("handle".to_string()).collect(),
    );
    index.trait_impls.insert(
        "crate::ports::Handler".to_string(),
        vec![impl_path.to_string()],
    );
    index
}

#[test]
fn trait_dispatch_emits_anchor_not_impl_edges() {
    // `dyn Handler` dispatch emits the synthetic trait-method anchor (never a
    // concrete impl edge, which would fabricate Check-C touchpoint fanout):
    // an unrelated method falls through to `<method>:name`; `+ Send` markers
    // and `Box<dyn …>` peeling don't change the anchor.
    // (label, src, impl_path, expected, forbidden)
    let cases: &[(&str, &str, &str, &str, &str)] = &[
        (
            "unrelated method falls through to <method>:",
            r#"
            use crate::ports::Handler;
            pub fn dispatch(h: &dyn Handler) {
                h.unrelated();
            }
            "#,
            "crate::app::X",
            "<method>:unrelated",
            "crate::app::X::unrelated",
        ),
        (
            "dyn Handler + Send marker → anchor",
            r#"
            use crate::ports::Handler;
            pub fn dispatch(h: &(dyn Handler + Send)) {
                h.handle();
            }
            "#,
            "crate::app::X",
            "crate::ports::Handler::handle",
            "crate::app::X::handle",
        ),
        (
            "Box<dyn Handler> peeled → anchor",
            r#"
            use crate::ports::Handler;
            pub fn dispatch(h: Box<dyn Handler>) {
                h.handle();
            }
            "#,
            "crate::app::Y",
            "crate::ports::Handler::handle",
            "crate::app::Y::handle",
        ),
    ];
    for (label, src, impl_path, expected, forbidden) in cases {
        let calls = run(&parse(src), &handler_index(impl_path), "dispatch");
        assert!(
            calls.contains(*expected),
            "case {label}: expected `{expected}` in {calls:?}"
        );
        assert!(
            !calls.contains(*forbidden),
            "case {label}: must not fabricate `{forbidden}` in {calls:?}"
        );
    }
}

#[test]
fn trait_dispatch_emits_anchor_regardless_of_default_status() {
    // `trait Handler { fn handle(&self) {} } impl Handler for AppHandler {}`
    // — the impl has no `handle` body and inherits the default. With
    // the trait-method anchor model, the dispatch still emits the
    // synthetic `<Trait>::<method>` anchor; whether the body lives in
    // the impl or the trait is no longer the dispatch site's problem
    // (the boundary walker treats the anchor as the capability the
    // adapter reaches). Concrete impl-edges are NEVER emitted from
    // dispatch — they would fabricate touchpoint fanout that fires
    // Check C false-positives.
    let fx = parse(
        r#"
        use crate::ports::Handler;
        pub fn dispatch(h: &dyn Handler) {
            h.handle();
        }
        "#,
    );
    let mut index = WorkspaceTypeIndex::new();
    index.trait_methods.insert(
        "crate::ports::Handler".to_string(),
        std::iter::once("handle".to_string()).collect(),
    );
    index.trait_impls.insert(
        "crate::ports::Handler".to_string(),
        vec!["crate::app::AppHandler".to_string()],
    );
    let mut by_impl: std::collections::HashMap<String, std::collections::HashSet<String>> =
        std::collections::HashMap::new();
    by_impl.insert(
        "crate::app::AppHandler".to_string(),
        std::collections::HashSet::new(),
    );
    index
        .trait_impl_overrides
        .insert("crate::ports::Handler".to_string(), by_impl);
    let calls = run(&fx, &index, "dispatch");
    assert!(
        calls.contains("crate::ports::Handler::handle"),
        "expected trait-method anchor edge, got {calls:?}"
    );
    assert!(
        !calls.contains("crate::app::AppHandler::handle"),
        "must not fabricate impl-method edge from dispatch, got {calls:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Stage 3: User-Wrapper-Config
// ═══════════════════════════════════════════════════════════════════
