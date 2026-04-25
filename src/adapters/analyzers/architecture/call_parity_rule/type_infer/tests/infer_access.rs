//! Tests for `infer_field`, `infer_try`, `infer_await`, `infer_cast`,
//! `infer_unary`, and the transparent Paren/Reference/Group wrappers.

use super::support::TypeInferFixture;
use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::{
    infer_type, CanonicalType,
};

fn infer(f: &TypeInferFixture, src: &str) -> Option<CanonicalType> {
    let expr: syn::Expr = syn::parse_str(src).ok()?;
    infer_type(&expr, &f.ctx(&f.file_scope()))
}

// ── Field access ─────────────────────────────────────────────────

#[test]
fn test_field_access_on_bound_struct() {
    let mut f = TypeInferFixture::new();
    f.bindings
        .insert("ctx", CanonicalType::path(["crate", "app", "Ctx"]));
    f.index.struct_fields.insert(
        ("crate::app::Ctx".to_string(), "session".to_string()),
        CanonicalType::path(["crate", "app", "Session"]),
    );
    let t = infer(&f, "ctx.session").expect("field resolved");
    assert_eq!(t, CanonicalType::path(["crate", "app", "Session"]));
}

#[test]
fn test_nested_field_access() {
    let mut f = TypeInferFixture::new();
    f.bindings
        .insert("ctx", CanonicalType::path(["crate", "app", "Ctx"]));
    f.index.struct_fields.insert(
        ("crate::app::Ctx".to_string(), "session".to_string()),
        CanonicalType::path(["crate", "app", "Session"]),
    );
    f.index.struct_fields.insert(
        ("crate::app::Session".to_string(), "id".to_string()),
        CanonicalType::path(["crate", "app", "Id"]),
    );
    let t = infer(&f, "ctx.session.id").expect("nested field resolved");
    assert_eq!(t, CanonicalType::path(["crate", "app", "Id"]));
}

#[test]
fn test_field_access_unknown_field_is_none() {
    let mut f = TypeInferFixture::new();
    f.bindings
        .insert("ctx", CanonicalType::path(["crate", "app", "Ctx"]));
    assert!(infer(&f, "ctx.missing").is_none());
}

#[test]
fn test_field_access_on_opaque_is_none() {
    let mut f = TypeInferFixture::new();
    f.bindings.insert("x", CanonicalType::Opaque);
    assert!(infer(&f, "x.field").is_none());
}

#[test]
fn test_tuple_field_is_none() {
    let mut f = TypeInferFixture::new();
    f.bindings
        .insert("x", CanonicalType::path(["crate", "app", "T"]));
    // Unnamed (tuple) members aren't indexed.
    assert!(infer(&f, "x.0").is_none());
}

// ── Try (?) ──────────────────────────────────────────────────────

#[test]
fn test_try_on_result_unwraps_ok() {
    let mut f = TypeInferFixture::new();
    f.bindings.insert(
        "res",
        CanonicalType::Result(Box::new(CanonicalType::path(["crate", "app", "T"]))),
    );
    let t = infer(&f, "res?").expect("try resolved");
    assert_eq!(t, CanonicalType::path(["crate", "app", "T"]));
}

#[test]
fn test_try_on_option_unwraps_some() {
    let mut f = TypeInferFixture::new();
    f.bindings.insert(
        "opt",
        CanonicalType::Option(Box::new(CanonicalType::path(["crate", "app", "T"]))),
    );
    let t = infer(&f, "opt?").expect("try on option");
    assert_eq!(t, CanonicalType::path(["crate", "app", "T"]));
}

#[test]
fn test_try_on_non_wrapper_is_none() {
    let mut f = TypeInferFixture::new();
    f.bindings
        .insert("x", CanonicalType::path(["crate", "app", "T"]));
    assert!(infer(&f, "x?").is_none());
}

// ── Await ────────────────────────────────────────────────────────

#[test]
fn test_await_on_future_unwraps_output() {
    let mut f = TypeInferFixture::new();
    f.bindings.insert(
        "fut",
        CanonicalType::Future(Box::new(CanonicalType::path(["crate", "app", "T"]))),
    );
    let t = infer(&f, "fut.await").expect("await resolved");
    assert_eq!(t, CanonicalType::path(["crate", "app", "T"]));
}

#[test]
fn test_await_on_result_is_none() {
    let mut f = TypeInferFixture::new();
    f.bindings.insert(
        "res",
        CanonicalType::Result(Box::new(CanonicalType::path(["crate", "app", "T"]))),
    );
    // .await on Result is a compile error — resolver stays strict and
    // returns None rather than unwrap it like ? would.
    assert!(infer(&f, "res.await").is_none());
}

// ── Cast ─────────────────────────────────────────────────────────

#[test]
fn test_cast_resolves_target_type() {
    let mut f = TypeInferFixture::new();
    f.bindings
        .insert("x", CanonicalType::path(["crate", "app", "Source"]));
    f.local_symbols.insert("Target".to_string());
    let t = infer(&f, "x as Target").expect("cast resolved");
    assert_eq!(t, CanonicalType::path(["crate", "app", "test", "Target"]));
}

#[test]
fn test_cast_to_unknown_is_none() {
    let mut f = TypeInferFixture::new();
    f.bindings
        .insert("x", CanonicalType::path(["crate", "app", "Source"]));
    assert!(infer(&f, "x as external::Unknown").is_none());
}

// ── Unary ────────────────────────────────────────────────────────

#[test]
fn test_deref_is_transparent() {
    let mut f = TypeInferFixture::new();
    f.bindings
        .insert("x", CanonicalType::path(["crate", "app", "T"]));
    let t = infer(&f, "*x").expect("deref resolved");
    assert_eq!(t, CanonicalType::path(["crate", "app", "T"]));
}

#[test]
fn test_negation_returns_none() {
    let mut f = TypeInferFixture::new();
    f.bindings
        .insert("x", CanonicalType::path(["crate", "app", "T"]));
    // `!x` yields bool, which we don't track.
    assert!(infer(&f, "!x").is_none());
}

#[test]
fn test_numeric_negation_returns_none() {
    let mut f = TypeInferFixture::new();
    f.bindings
        .insert("x", CanonicalType::path(["crate", "app", "T"]));
    // `-x` — numeric, not tracked.
    assert!(infer(&f, "-x").is_none());
}

// ── Transparent wrappers: Paren, Reference, Group ────────────────

#[test]
fn test_paren_is_transparent() {
    let mut f = TypeInferFixture::new();
    f.bindings
        .insert("x", CanonicalType::path(["crate", "app", "T"]));
    let t = infer(&f, "(x)").expect("paren resolved");
    assert_eq!(t, CanonicalType::path(["crate", "app", "T"]));
}

#[test]
fn test_reference_strips() {
    let mut f = TypeInferFixture::new();
    f.bindings
        .insert("x", CanonicalType::path(["crate", "app", "T"]));
    let t = infer(&f, "&x").expect("ref resolved");
    assert_eq!(t, CanonicalType::path(["crate", "app", "T"]));
}

#[test]
fn test_mutable_reference_strips() {
    let mut f = TypeInferFixture::new();
    f.bindings
        .insert("x", CanonicalType::path(["crate", "app", "T"]));
    let t = infer(&f, "&mut x").expect("mut ref resolved");
    assert_eq!(t, CanonicalType::path(["crate", "app", "T"]));
}

// ── Unsupported expression forms return None ─────────────────────

#[test]
fn test_closure_expression_is_none() {
    let f = TypeInferFixture::new();
    // Closures aren't tracked in Stage 1.
    assert!(infer(&f, "|x| x").is_none());
}

#[test]
fn test_if_expression_is_none() {
    let f = TypeInferFixture::new();
    // Conditional expressions aren't tracked in Stage 1 (need unification).
    assert!(infer(&f, "if true { 1 } else { 2 }").is_none());
}

#[test]
fn test_literal_expression_is_none() {
    let f = TypeInferFixture::new();
    // Literals produce primitive types we don't track.
    assert!(infer(&f, "42").is_none());
}
