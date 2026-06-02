use super::*;

// ── Receiver-Type-Tracking ────────────────────────────────────

const RLM_USE: &str = "use crate::app::session::RlmSession;";
/// Receiver-tracking case: `(label, source, importing_file, fn)`. `String`
/// source (built with `format!`) so the shared `use` prefix can be reused.
type ReceiverCase = (&'static str, String, &'static str, &'static str);

/// Cases where the `RlmSession` receiver type is carried by a function
/// parameter (by value, by ref, `Arc`-wrapped, and async).
fn receiver_param_cases() -> Vec<ReceiverCase> {
    vec![
        (
            "fn param by value",
            format!("{RLM_USE}\npub fn handle(session: RlmSession) {{ session.search(1); }}"),
            "src/mcp/handlers.rs",
            "handle",
        ),
        (
            "fn param by ref",
            format!("{RLM_USE}\npub fn handle(session: &RlmSession) {{ session.search(1); }}"),
            "src/mcp/handlers.rs",
            "handle",
        ),
        (
            "fn param Arc-wrapped",
            format!("{RLM_USE}\nuse std::sync::Arc;\npub fn handle(session: Arc<RlmSession>) {{ session.search(1); }}"),
            "src/mcp/handlers.rs",
            "handle",
        ),
        (
            "async fn",
            format!("{RLM_USE}\npub async fn handle(s: RlmSession) {{ s.search(1).await; }}"),
            "src/mcp/handlers.rs",
            "handle",
        ),
    ]
}

/// Cases where the receiver type comes from a local binding (constructor,
/// type annotation, alias-resolved constructor, closure capture).
fn receiver_binding_cases() -> Vec<ReceiverCase> {
    vec![
        (
            "let-binding constructor",
            format!("{RLM_USE}\npub fn cmd_search(q: u32) {{ let s = RlmSession::open_cwd(); s.search(q); }}"),
            "src/cli/handlers.rs",
            "cmd_search",
        ),
        (
            "let type annotation",
            format!("{RLM_USE}\npub fn cmd_search(q: u32) {{ let s: RlmSession = make_session(); s.search(q); }}"),
            "src/cli/handlers.rs",
            "cmd_search",
        ),
        (
            "alias-resolved constructor",
            format!("{RLM_USE}\npub fn cmd_search() {{ let s = RlmSession::open(); s.search(1); }}"),
            "src/cli/handlers.rs",
            "cmd_search",
        ),
        (
            "closure inherits parent binding",
            format!("{RLM_USE}\npub fn cmd_search() {{ let s = RlmSession::open(); let f = || s.search(1); f(); }}"),
            "src/cli/handlers.rs",
            "cmd_search",
        ),
    ]
}

#[test]
fn tracker_resolves_receiver_type() {
    // However a binding/param of `RlmSession` is introduced, `s.search()`
    // must resolve to `RlmSession::search`.
    let mut cases = receiver_param_cases();
    cases.extend(receiver_binding_cases());
    for (label, src, path, fn_name) in &cases {
        let calls = calls_in(src, path, fn_name);
        assert!(
            calls.contains("crate::app::session::RlmSession::search"),
            "case {label}: {calls:?}"
        );
    }
}

#[test]
fn test_tracker_fn_param_box_ref_mut_type() {
    let fctx = load(
        r#"
        use crate::app::session::RlmSession;
        pub fn a(session: Box<RlmSession>) { session.search(1); }
        pub fn b(session: &mut RlmSession) { session.search(1); }
        "#,
    );
    let fs = fctx.file_scope("src/mcp/handlers.rs");
    for name in &["a", "b"] {
        let ctx = ctx_for_fn(&fctx, &fs, name);
        let calls = collect_canonical_calls(&ctx);
        assert!(
            calls.contains("crate::app::session::RlmSession::search"),
            "fn {name} calls = {:?}",
            calls
        );
    }
}

#[test]
fn test_tracker_shadowing_uses_latest() {
    let calls = calls_in(
        r#"
        use crate::app::session::RlmSession;
        use crate::cli::CliSession;
        pub fn cmd_search() {
            let s = CliSession::new();
            let s = RlmSession::open();
            s.search(1);
        }
        "#,
        "src/cli/handlers.rs",
        "cmd_search",
    );
    assert!(calls.contains("crate::app::session::RlmSession::search"));
    assert!(!calls.contains("crate::cli::CliSession::search"));
}

#[test]
fn test_tracker_unknown_receiver_falls_back_to_method_shape() {
    let calls = calls_in(
        r#"
        pub fn cmd_search(x: UnknownType) {
            x.search(1);
        }
        "#,
        "src/cli/handlers.rs",
        "cmd_search",
    );
    assert!(calls.contains("<method>:search"));
    assert!(!calls.iter().any(|c| c.contains("UnknownType::search")));
}

#[test]
fn test_tracker_factory_helper_unresolved_falls_back_to_method_shape() {
    // Documented limitation: no 1-hop return-type inference.
    let calls = calls_in(
        r#"
        pub fn cmd_search() {
            let s = helpers::open_session();
            s.search(1);
        }
        "#,
        "src/cli/handlers.rs",
        "cmd_search",
    );
    assert!(calls.contains("<method>:search"));
}
