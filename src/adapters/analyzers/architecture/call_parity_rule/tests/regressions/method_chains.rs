use super::*;

#[test]
fn method_chain_ctor_open_map_err_unwrap_resolves_receiver() {
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            let s = Session::open().map_err(handle).unwrap();
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "cmd");
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "expected Session::diff edge, got {calls:?}"
    );
}

#[test]
fn method_chain_ctor_open_cwd_map_err_try_resolves_receiver() {
    // The exact pattern that motivated this inference work:
    // `let session = open_cwd().map_err(f)?` then `session.method()`.
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            let s = Session::open_cwd().map_err(map_err)?;
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "cmd");
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "expected Session::diff edge, got {calls:?}"
    );
}

#[test]
fn method_chain_ctor_plain_unwrap_resolves_receiver() {
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            let s = Session::open().unwrap();
            s.files();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "cmd");
    assert!(calls.contains("crate::app::session::Session::files"));
}

#[test]
fn method_chain_ctor_expect_message_resolves_receiver() {
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            let s = Session::open().expect("session must open");
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "cmd");
    assert!(calls.contains("crate::app::session::Session::diff"));
}

#[test]
fn method_chain_ctor_unwrap_or_else_closure_resolves_receiver() {
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            let s = Session::open().unwrap_or_else(|e| fallback(e));
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "cmd");
    assert!(calls.contains("crate::app::session::Session::diff"));
}

#[test]
fn method_chain_ctor_chained_inline_call_resolves_receiver() {
    // No intermediate `let` — the chain resolves inside a single
    // method-call expression.
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            Session::open().unwrap().diff();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "cmd");
    assert!(calls.contains("crate::app::session::Session::diff"));
}

#[test]
fn method_chain_ctor_insert_returning_result_chained_resolves_receiver() {
    // Session::insert returns Result<Response, _> — verify the outer
    // call edge is recorded even on a Result-wrapped receiver chain.
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            Session::open().unwrap().insert();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "cmd");
    assert!(calls.contains("crate::app::session::Session::insert"));
}

// ═══════════════════════════════════════════════════════════════════
// Positive: cascading struct-field access patterns
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cascading_struct_field_access_resolves_receiver() {
    let fx = parse(
        r#"
        use crate::app::Ctx;
        pub fn handle(ctx: &Ctx) {
            ctx.session.diff();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "handle");
    assert!(calls.contains("crate::app::session::Session::diff"));
}

#[test]
fn cascading_struct_field_access_via_let_binding_resolves_receiver() {
    let fx = parse(
        r#"
        use crate::app::Ctx;
        pub fn handle(ctx: &Ctx) {
            let s = &ctx.session;
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "handle");
    // `&ctx.session` inferred as Session (Reference is transparent).
    assert!(calls.contains("crate::app::session::Session::diff"));
}

// ═══════════════════════════════════════════════════════════════════
// Positive: self receiver inside impl methods
// ═══════════════════════════════════════════════════════════════════
