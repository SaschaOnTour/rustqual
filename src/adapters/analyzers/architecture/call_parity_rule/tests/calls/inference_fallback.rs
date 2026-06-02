use super::*;

// ── Shallow-inference fallback (Task 1.6) ────────────────────────

/// Helper: build a `FnContext` with a pre-populated workspace index.
fn ctx_with_index<'a>(
    fctx: &'a FileCtx,
    file_scope: &'a FileScope<'a>,
    fn_name: &str,
    index: &'a crate::adapters::analyzers::architecture::call_parity_rule::type_infer::WorkspaceTypeIndex,
) -> FnContext<'a> {
    let f = find_fn(&fctx.file, fn_name);
    FnContext {
        file: file_scope,
        mod_stack: &[],
        body: &f.block,
        signature_params: sig_params(&f.sig),
        generic_params: std::collections::HashMap::new(),
        self_type: None,
        workspace_index: Some(index),
        workspace_files: None,
        reexports: None,
    }
}

#[test]
fn test_inference_fallback_resolves_session_diff_through_open_result() {
    // `Session::open()` returns Result<Session, _>; inference unwraps the
    // combinator chain (legacy extract_let_binding can't see through the
    // MethodCall) and resolves the trailing `.diff()` to Session::diff,
    // however the chain is spelled. (label, source, fn)
    use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::{
        CanonicalType, WorkspaceTypeIndex,
    };
    let cases: &[(&str, &str, &str)] = &[
        (
            "method chain ctor + map_err + unwrap via let binding",
            r#"
        use crate::app::session::Session;
        pub fn cmd_diff() {
            let session = Session::open().map_err(handle_err).unwrap();
            session.diff();
        }
        "#,
            "cmd_diff",
        ),
        (
            "direct open().unwrap().diff() combinator chain",
            r#"
        use crate::app::session::Session;
        pub fn cmd_direct() {
            Session::open().unwrap().diff();
        }
        "#,
            "cmd_direct",
        ),
    ];
    for (label, src, fn_name) in cases {
        let fctx = load(src);
        let mut index = WorkspaceTypeIndex::new();
        index.insert_method_return(
            "crate::app::session::Session",
            "open",
            CanonicalType::Result(Box::new(CanonicalType::path([
                "crate", "app", "session", "Session",
            ]))),
        );
        let fs = fctx.file_scope("src/cli/handlers.rs");
        let ctx = ctx_with_index(&fctx, &fs, fn_name, &index);
        let calls = collect_canonical_calls(&ctx);
        assert!(
            calls.contains("crate::app::session::Session::diff"),
            "case {label}: {calls:?}"
        );
    }
}

#[test]
fn test_inference_fallback_resolves_field_access() {
    // `ctx.session.diff()` — receiver is Expr::Field, resolved via the
    // inference layer + workspace struct-field index. Fixture uses
    // `use crate::app::Ctx` so the signature-param `&Ctx` canonicalises
    // to `crate::app::Ctx` directly.
    use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::{
        CanonicalType, WorkspaceTypeIndex,
    };
    let fctx = load(
        r#"
        use crate::app::Ctx;
        pub fn handle_diff(ctx: &Ctx) {
            ctx.session.diff();
        }
        "#,
    );
    let mut index = WorkspaceTypeIndex::new();
    index.insert_struct_field(
        "crate::app::Ctx",
        "session",
        CanonicalType::path(["crate", "app", "Session"]),
    );
    let fs = fctx.file_scope("src/cli/handlers.rs");
    let ctx = ctx_with_index(&fctx, &fs, "handle_diff", &index);
    let calls = collect_canonical_calls(&ctx);
    assert!(
        calls.contains("crate::app::Session::diff"),
        "field-access inference should resolve ctx.session.diff(), got {calls:?}"
    );
}

#[test]
fn test_existing_fast_path_still_works_without_index() {
    // Regression guard: legacy extract_let_binding keeps working when
    // workspace_index is None (unit-test fixture shape).
    let calls = calls_in(
        r#"
        use crate::app::session::RlmSession;
        pub fn cmd_search(q: u32) {
            let s = RlmSession::open_cwd();
            s.search(q);
        }
        "#,
        "src/cli/handlers.rs",
        "cmd_search",
    );
    assert!(calls.contains("crate::app::session::RlmSession::search"));
}
