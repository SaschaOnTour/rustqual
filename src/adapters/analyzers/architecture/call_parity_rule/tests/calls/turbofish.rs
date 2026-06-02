use super::*;

// ── Turbofish + inferred-T resolution (v1.2.1) ────────────────────

#[test]
fn test_collect_turbofish_call_resolves_via_use() {
    // `record_symbol_query::<RefsQuery>(args)` with a `use` import in
    // scope → the call must canonicalise to the absolute path of the
    // imported function, not `<bare>:`. Pre-fix, turbofish call sites
    // were dropped from the call graph.
    let calls = calls_in(
        r#"
        use crate::middleware::record_symbol_query;
        pub fn cmd_search() {
            record_symbol_query::<RefsQuery>(0);
        }
        "#,
        "src/cli/handlers.rs",
        "cmd_search",
    );
    assert!(
        calls.contains("crate::middleware::record_symbol_query"),
        "turbofish call must resolve through use-alias, got {calls:?}"
    );
}

#[test]
fn test_collect_inferred_generic_call_resolves_via_use() {
    // `record_operation(args)` (T inferred) with use in scope. Same
    // resolution path as a non-generic call — confirms there's no
    // generic-args-keying mismatch on the definition side.
    let calls = calls_in(
        r#"
        use crate::middleware::record_operation;
        pub fn cmd_search() {
            record_operation(0, "x", true);
        }
        "#,
        "src/cli/handlers.rs",
        "cmd_search",
    );
    assert!(
        calls.contains("crate::middleware::record_operation"),
        "inferred-generic call must resolve through use-alias, got {calls:?}"
    );
}

#[test]
fn test_turbofish_in_impl_method_body_resolves() {
    // Turbofish call lives inside an impl method, not a free fn. The
    // impl block's self_ty has its own scope; the `use` must still be
    // visible in the body.
    let calls = calls_in_impl_method(
        r#"
        use crate::middleware::record_symbol_query;
        pub struct Session;
        impl Session {
            pub fn refs(&self, sym: &str) {
                record_symbol_query::<RefsQuery>(&self, sym);
            }
        }
        "#,
        "src/application/session.rs",
        "Session",
        "refs",
    );
    assert!(
        calls.contains("crate::middleware::record_symbol_query"),
        "turbofish in impl-method body must resolve via use, got {calls:?}"
    );
}

#[test]
fn test_turbofish_via_child_module_qualified_path_resolves() {
    // Pattern that may bite when use is in a sibling/child module:
    // qualified-path call with turbofish, no `use` at the call site.
    let fctx = load_with_roots(
        r#"
        pub fn cmd_search() {
            crate::middleware::record_symbol_query::<RefsQuery>(0);
        }
        "#,
        &["middleware"],
    );
    let fs = fctx.file_scope("src/cli/handlers.rs");
    let ctx = ctx_for_fn(&fctx, &fs, "cmd_search");
    let calls = collect_canonical_calls(&ctx);
    assert!(
        calls.contains("crate::middleware::record_symbol_query"),
        "qualified-path turbofish must resolve, got {calls:?}"
    );
}

#[test]
fn test_multiple_turbofish_calls_to_same_fn_resolve_to_one_canonical() {
    // Three turbofish call sites, three different generic args.
    // The collector must dedupe to a single canonical entry (the call
    // set is a HashSet<String>).
    let calls = calls_in_impl_method(
        r#"
        use crate::middleware::record_symbol_query;
        pub struct Session;
        impl Session {
            pub fn refs(&self, sym: &str) {
                record_symbol_query::<RefsQuery>(&self, sym);
                record_symbol_query::<ContextQuery>(&self, sym);
                record_symbol_query::<ContextWithGraphQuery>(&self, sym);
            }
        }
        "#,
        "src/application/session.rs",
        "Session",
        "refs",
    );
    assert!(
        calls.contains("crate::middleware::record_symbol_query"),
        "all three turbofish call sites must dedupe to one canonical, got {calls:?}"
    );
}

#[test]
fn test_turbofish_unresolved_path_still_bare() {
    // No `use` for `Box` → still `<bare>:Box::new`. Behavior unchanged
    // for genuinely unresolvable paths; only the resolved case needed
    // fixing.
    let calls = calls_in(
        r#"
        pub fn cmd_search() {
            Box::<u32>::new(42);
        }
        "#,
        "src/cli/handlers.rs",
        "cmd_search",
    );
    assert!(
        calls.contains("<bare>:Box::new"),
        "unresolved turbofish should still bare-key, got {calls:?}"
    );
}

#[test]
fn test_collect_closure_body_collected() {
    let calls = calls_in(
        r#"
        pub fn cmd_search() {
            let f = |x: u32| inner_call(x);
            f(1);
        }
        "#,
        "src/cli/handlers.rs",
        "cmd_search",
    );
    assert!(calls.contains("<bare>:inner_call"));
}

#[test]
fn test_collect_await_is_not_extra_call() {
    let calls = calls_in(
        r#"
        pub async fn cmd_search() {
            f(1).await;
        }
        "#,
        "src/cli/handlers.rs",
        "cmd_search",
    );
    assert!(calls.contains("<bare>:f"));
    // .await is not a call target
    assert!(!calls.iter().any(|c| c.contains("await")));
}

#[test]
fn test_collect_self_dispatch_in_impl() {
    let calls = calls_in_impl_method(
        r#"
        pub struct RlmSession;
        impl RlmSession {
            pub fn search(&self) {
                Self::internal_helper();
            }
        }
        "#,
        "src/application/session.rs",
        "RlmSession",
        "search",
    );
    assert!(
        calls.contains("crate::application::session::RlmSession::internal_helper"),
        "calls = {:?}",
        calls
    );
}
