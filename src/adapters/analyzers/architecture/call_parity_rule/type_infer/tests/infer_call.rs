//! Tests for `infer_path_expr`, `infer_call`, and `infer_method_call`.

use super::support::TypeInferFixture;
use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::{
    infer_type, CanonicalType,
};
use crate::adapters::shared::use_tree::AliasTarget;

fn infer(f: &TypeInferFixture, src: &str) -> Option<CanonicalType> {
    let expr: syn::Expr = syn::parse_str(src).ok()?;
    infer_type(&expr, &f.ctx(&f.file_scope()))
}

// ── Path expressions (bare idents) ───────────────────────────────

#[test]
fn test_bare_ident_resolves_from_bindings() {
    let mut f = TypeInferFixture::new();
    f.bindings
        .insert("session", CanonicalType::path(["crate", "app", "Session"]));
    let t = infer(&f, "session").expect("bound ident");
    assert_eq!(t, CanonicalType::path(["crate", "app", "Session"]));
}

#[test]
fn test_bare_ident_not_in_bindings_is_none() {
    let f = TypeInferFixture::new();
    assert!(infer(&f, "unknown").is_none());
}

#[test]
fn test_multi_segment_path_expr_is_none() {
    let f = TypeInferFixture::new();
    // `crate::foo::BAR` as a standalone expression is a const/static ref
    // which we don't track in Stage 1.
    assert!(infer(&f, "crate::foo::BAR").is_none());
}

// ── Call: free fn ────────────────────────────────────────────────

#[test]
fn test_call_single_ident_resolves_via_fn_returns() {
    let mut f = TypeInferFixture::new();
    f.local_symbols.insert("make_session".to_string());
    f.index.fn_returns.insert(
        "crate::app::test::make_session".to_string(),
        CanonicalType::path(["crate", "app", "Session"]),
    );
    let t = infer(&f, "make_session()").expect("fn resolved");
    assert_eq!(t, CanonicalType::path(["crate", "app", "Session"]));
}

#[test]
fn test_call_crate_path_resolves_via_fn_returns() {
    let mut f = TypeInferFixture::new();
    f.index.fn_returns.insert(
        "crate::app::make_session".to_string(),
        CanonicalType::path(["crate", "app", "Session"]),
    );
    let t = infer(&f, "crate::app::make_session()").expect("fn resolved");
    assert_eq!(t, CanonicalType::path(["crate", "app", "Session"]));
}

#[test]
fn test_call_unknown_fn_is_none() {
    let f = TypeInferFixture::new();
    assert!(infer(&f, "unknown_fn()").is_none());
}

// ── Call: associated fn (T::ctor) ─────────────────────────────────

#[test]
fn test_call_type_ctor_resolves_via_method_returns() {
    let mut f = TypeInferFixture::new();
    f.local_symbols.insert("Session".to_string());
    f.index.insert_method_return(
        "crate::app::test::Session",
        "open",
        CanonicalType::path(["crate", "app", "test", "Session"]),
    );
    let t = infer(&f, "Session::open()").expect("assoc fn resolved");
    assert_eq!(t, CanonicalType::path(["crate", "app", "test", "Session"]));
}

#[test]
fn test_call_ctor_via_alias() {
    let mut f = TypeInferFixture::new();
    f.alias_map.insert(
        "Session".to_string(),
        AliasTarget::relative(vec![
            "crate".to_string(),
            "app".to_string(),
            "session".to_string(),
            "Session".to_string(),
        ]),
    );
    f.index.insert_method_return(
        "crate::app::session::Session",
        "new",
        CanonicalType::path(["crate", "app", "session", "Session"]),
    );
    let t = infer(&f, "Session::new()").expect("alias resolved");
    assert_eq!(
        t,
        CanonicalType::path(["crate", "app", "session", "Session"])
    );
}

#[test]
fn test_call_ctor_returning_result() {
    let mut f = TypeInferFixture::new();
    f.local_symbols.insert("Session".to_string());
    f.index.insert_method_return(
        "crate::app::test::Session",
        "open",
        CanonicalType::Result(Box::new(CanonicalType::path([
            "crate", "app", "test", "Session",
        ]))),
    );
    let t = infer(&f, "Session::open()").expect("ctor resolved");
    assert!(matches!(t, CanonicalType::Result(_)));
}

// ── Call: Self:: substitution ────────────────────────────────────

#[test]
fn test_call_self_substitutes_to_impl_type() {
    let mut f = TypeInferFixture::new();
    f.self_type = Some(vec![
        "crate".to_string(),
        "app".to_string(),
        "Session".to_string(),
    ]);
    f.index.insert_method_return(
        "crate::app::Session",
        "new",
        CanonicalType::path(["crate", "app", "Session"]),
    );
    let t = infer(&f, "Self::new()").expect("Self::new resolved");
    assert_eq!(t, CanonicalType::path(["crate", "app", "Session"]));
}

#[test]
fn test_call_self_without_self_type_is_none() {
    let f = TypeInferFixture::new();
    assert!(infer(&f, "Self::new()").is_none());
}

// ── MethodCall ────────────────────────────────────────────────────

#[test]
fn method_call_resolves_receiver_type_through_references() {
    // A method call resolves via the receiver's bound type. A parenthesised
    // reference receiver `(&s)` must be stripped first, then resolved the same
    // way as a direct receiver.
    for (label, var, expr) in [
        ("direct receiver", "session", "session.diff()"),
        ("reference receiver is stripped", "s", "(&s).diff()"),
    ] {
        let mut f = TypeInferFixture::new();
        f.bindings
            .insert(var, CanonicalType::path(["crate", "app", "Session"]));
        f.index.insert_method_return(
            "crate::app::Session",
            "diff",
            CanonicalType::path(["crate", "app", "Response"]),
        );
        let t = infer(&f, expr).unwrap_or_else(|| panic!("case {label}: method resolved"));
        assert_eq!(
            t,
            CanonicalType::path(["crate", "app", "Response"]),
            "case {label}"
        );
    }
}

#[test]
fn test_method_call_chained_via_fn_return() {
    let mut f = TypeInferFixture::new();
    f.local_symbols.insert("make_session".to_string());
    f.index.fn_returns.insert(
        "crate::app::test::make_session".to_string(),
        CanonicalType::path(["crate", "app", "Session"]),
    );
    f.index.insert_method_return(
        "crate::app::Session",
        "diff",
        CanonicalType::path(["crate", "app", "Response"]),
    );
    let t = infer(&f, "make_session().diff()").expect("chain resolved");
    assert_eq!(t, CanonicalType::path(["crate", "app", "Response"]));
}

#[test]
fn test_method_call_stdlib_combinator_resolves() {
    let mut f = TypeInferFixture::new();
    f.bindings.insert(
        "res",
        CanonicalType::Result(Box::new(CanonicalType::path(["crate", "app", "T"]))),
    );
    // `.unwrap()` on Result<T,_> → T via the combinator table.
    let t = infer(&f, "res.unwrap()").expect("unwrap resolved");
    assert_eq!(t, CanonicalType::path(["crate", "app", "T"]));
}

#[test]
fn test_method_call_closure_combinator_is_none() {
    let mut f = TypeInferFixture::new();
    f.bindings.insert(
        "res",
        CanonicalType::Result(Box::new(CanonicalType::path(["crate", "app", "T"]))),
    );
    // `.map(|x| ...)` depends on closure body — unresolved by design.
    assert!(infer(&f, "res.map(|x| x)").is_none());
}

#[test]
fn test_method_call_on_opaque_receiver_is_none() {
    let mut f = TypeInferFixture::new();
    f.bindings.insert("x", CanonicalType::Opaque);
    assert!(infer(&f, "x.method()").is_none());
}

#[test]
fn test_method_call_unknown_method_is_none() {
    let mut f = TypeInferFixture::new();
    f.bindings
        .insert("x", CanonicalType::path(["crate", "app", "Session"]));
    // No entry in method_returns for "bogus" on Session.
    assert!(infer(&f, "x.bogus()").is_none());
}

// ── try_method_return: ≥3-segment type-qualified calls ───────────

#[test]
fn type_qualified_assoc_call_resolves_via_method_returns() {
    // A 3+-segment `crate::app::Session::open()` call splits the last
    // segment as the method and probes `method_returns` for the type
    // prefix. Pins `try_method_return`'s `segs.len() < 2` guard against
    // `>` (which would bail on the 4-segment path and return None).
    let mut f = TypeInferFixture::new();
    f.index.insert_method_return(
        "crate::app::Session",
        "open",
        CanonicalType::path(["crate", "app", "Session"]),
    );
    let t = infer(&f, "crate::app::Session::open()").expect("3-seg assoc call resolved");
    assert_eq!(t, CanonicalType::path(["crate", "app", "Session"]));
}

// ── lookup_trait_method_return: dyn-Trait receiver dispatch ───────

#[test]
fn trait_bound_receiver_resolves_via_impl_return() {
    // A receiver typed as a trait bound dispatches `.handle()` to the
    // first workspace impl's return type. Pins both
    // `lookup_trait_method_return -> None` and its `!trait_has_method`
    // guard (delete `!`) — either would lose the resolution.
    let mut f = TypeInferFixture::new();
    f.bindings.insert(
        "h",
        CanonicalType::TraitBound(vec![vec![
            "crate".to_string(),
            "app".to_string(),
            "Handler".to_string(),
        ]]),
    );
    f.index.trait_methods.insert(
        "crate::app::Handler".to_string(),
        std::collections::HashSet::from(["handle".to_string()]),
    );
    f.index.trait_impls.insert(
        "crate::app::Handler".to_string(),
        vec!["crate::app::LogHandler".to_string()],
    );
    f.index.insert_method_return(
        "crate::app::LogHandler",
        "handle",
        CanonicalType::path(["crate", "app", "Outcome"]),
    );
    let t = infer(&f, "h.handle()").expect("trait-bound method dispatch resolved");
    assert_eq!(t, CanonicalType::path(["crate", "app", "Outcome"]));
}
