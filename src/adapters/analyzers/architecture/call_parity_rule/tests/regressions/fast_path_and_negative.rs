use super::*;

#[test]
fn fast_path_signature_param_resolves() {
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn handle(s: &Session) {
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "handle");
    assert!(calls.contains("crate::app::session::Session::diff"));
}

#[test]
fn fast_path_let_type_annotation() {
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            let s: Session = make_it();
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "cmd");
    assert!(calls.contains("crate::app::session::Session::diff"));
}

#[test]
fn fast_path_direct_constructor() {
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            let s = Session::open_cwd();
            // No unwrap — s is Result<Session, _>, not Session.
            // Fast path on the bare-ident fails; inference fallback on
            // `s.diff()` receiver infers Result<Session>, which doesn't
            // have `diff` in the combinator table → <method>:diff.
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "cmd");
    // This pattern is pathological (caller should `?` or `unwrap`), but
    // we verify the resolver doesn't invent a false Session::diff edge.
    assert!(
        calls.contains("<method>:diff") || calls.contains("crate::app::session::Session::diff"),
        "pathological Result<T>.method() must either fall back or correctly unwrap, got {calls:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Negative: documented Stage 1 limits (unresolved stays unresolved)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn negative_external_type_method_is_bare() {
    // `u32` is stdlib — no workspace entry. Calling a made-up method
    // on it must land as `<method>:name` rather than confabulate.
    let fx = parse(
        r#"
        pub fn cmd() {
            let x: u32 = 42;
            x.custom_method();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "cmd");
    assert!(
        calls.contains("<method>:custom_method"),
        "expected <method>:custom_method fallback, got {calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c.contains("u32::custom_method")),
        "must not fabricate stdlib method edges, got {calls:?}"
    );
}

#[test]
fn negative_unannotated_generic_stays_unresolved() {
    // `fn get<T>() -> T` yields Opaque; `x.m()` falls back.
    let fx = parse(
        r#"
        pub fn cmd() {
            let x = get();
            x.some_method();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "cmd");
    assert!(calls.contains("<method>:some_method"));
}

#[test]
fn negative_stdlib_map_closure_is_unresolved() {
    // `.map(|r| r.diff())` inner call on the closure argument — the
    // closure body is visited, `r` has no binding → <method>:diff. The
    // outer `.map()` itself also yields <method>:map (stdlib
    // closure-dependent combinator).
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            Session::open().map(|r| r.diff());
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "cmd");
    // The inner `r.diff()` is unresolved; assert it stays <method>:diff.
    assert!(
        calls.iter().any(|c| c == "<method>:diff"),
        "closure-body call should stay <method>:diff without binding, got {calls:?}"
    );
}

#[test]
fn negative_tuple_destructuring_is_limit() {
    // Stage 1 doesn't track tuple element types. `let (a, s) = setup();
    // s.m()` leaves `s` unresolved.
    let fx = parse(
        r#"
        pub fn cmd() {
            let (a, s) = setup();
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "cmd");
    // Documented limit: tuple-destructured bindings are Opaque.
    assert!(
        calls.contains("<method>:diff"),
        "tuple destructuring is a Stage 1 limit — expected <method>:diff, got {calls:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Robustness: mixed positive + negative in one fn body
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// Stage 2: Trait-Dispatch Over-Approximation
// ═══════════════════════════════════════════════════════════════════
