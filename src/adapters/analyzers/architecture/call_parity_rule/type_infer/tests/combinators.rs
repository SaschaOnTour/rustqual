//! Tests for the stdlib-combinator return-type table.
//!
//! Each stdlib wrapper gets positive tests (method resolves to expected
//! return type) and negative tests (closure-dependent methods stay
//! unresolved).

use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::{
    combinator_return, CanonicalType,
};

fn t() -> CanonicalType {
    CanonicalType::path(["crate", "app", "T"])
}

// ── Result<T, E> ─────────────────────────────────────────────────

#[test]
fn test_result_unwrap_yields_t() {
    let res = CanonicalType::Result(Box::new(t()));
    assert_eq!(combinator_return(&res, "unwrap"), Some(t()));
}

#[test]
fn test_result_expect_yields_t() {
    let res = CanonicalType::Result(Box::new(t()));
    assert_eq!(combinator_return(&res, "expect"), Some(t()));
}

#[test]
fn test_result_unwrap_or_yields_t() {
    let res = CanonicalType::Result(Box::new(t()));
    assert_eq!(combinator_return(&res, "unwrap_or"), Some(t()));
    assert_eq!(combinator_return(&res, "unwrap_or_else"), Some(t()));
    assert_eq!(combinator_return(&res, "unwrap_or_default"), Some(t()));
}

#[test]
fn test_result_ok_yields_option_t() {
    let res = CanonicalType::Result(Box::new(t()));
    assert_eq!(
        combinator_return(&res, "ok"),
        Some(CanonicalType::Option(Box::new(t())))
    );
}

#[test]
fn test_result_err_yields_option_opaque() {
    let res = CanonicalType::Result(Box::new(t()));
    assert_eq!(
        combinator_return(&res, "err"),
        Some(CanonicalType::Option(Box::new(CanonicalType::Opaque)))
    );
}

#[test]
fn test_result_map_err_preserves_ok_type() {
    let res = CanonicalType::Result(Box::new(t()));
    assert_eq!(
        combinator_return(&res, "map_err"),
        Some(CanonicalType::Result(Box::new(t())))
    );
}

#[test]
fn test_result_or_else_preserves_ok_type() {
    let res = CanonicalType::Result(Box::new(t()));
    assert_eq!(
        combinator_return(&res, "or_else"),
        Some(CanonicalType::Result(Box::new(t())))
    );
}

#[test]
fn test_result_map_is_unresolved() {
    // `.map(|x| ...)` depends on the closure — unresolved by design.
    let res = CanonicalType::Result(Box::new(t()));
    assert_eq!(combinator_return(&res, "map"), None);
}

#[test]
fn test_result_and_then_is_unresolved() {
    let res = CanonicalType::Result(Box::new(t()));
    assert_eq!(combinator_return(&res, "and_then"), None);
}

#[test]
fn test_result_unknown_method_is_none() {
    let res = CanonicalType::Result(Box::new(t()));
    assert_eq!(combinator_return(&res, "totally_made_up"), None);
}

// ── Option<T> ────────────────────────────────────────────────────

#[test]
fn test_option_unwrap_yields_t() {
    let opt = CanonicalType::Option(Box::new(t()));
    assert_eq!(combinator_return(&opt, "unwrap"), Some(t()));
}

#[test]
fn test_option_unwrap_or_yields_t() {
    let opt = CanonicalType::Option(Box::new(t()));
    assert_eq!(combinator_return(&opt, "unwrap_or"), Some(t()));
    assert_eq!(combinator_return(&opt, "unwrap_or_else"), Some(t()));
    assert_eq!(combinator_return(&opt, "unwrap_or_default"), Some(t()));
}

#[test]
fn test_option_ok_or_yields_result_t() {
    let opt = CanonicalType::Option(Box::new(t()));
    assert_eq!(
        combinator_return(&opt, "ok_or"),
        Some(CanonicalType::Result(Box::new(t())))
    );
    assert_eq!(
        combinator_return(&opt, "ok_or_else"),
        Some(CanonicalType::Result(Box::new(t())))
    );
}

#[test]
fn test_option_preserve_wrapper_methods() {
    let opt = CanonicalType::Option(Box::new(t()));
    for method in [
        "or", "or_else", "filter", "take", "replace", "as_ref", "as_mut", "cloned", "copied",
    ] {
        assert_eq!(
            combinator_return(&opt, method),
            Some(CanonicalType::Option(Box::new(t()))),
            "method: {}",
            method
        );
    }
}

#[test]
fn test_option_map_is_unresolved() {
    let opt = CanonicalType::Option(Box::new(t()));
    assert_eq!(combinator_return(&opt, "map"), None);
}

#[test]
fn test_option_and_then_is_unresolved() {
    let opt = CanonicalType::Option(Box::new(t()));
    assert_eq!(combinator_return(&opt, "and_then"), None);
}

// ── Non-wrapper receivers ────────────────────────────────────────

#[test]
fn test_path_receiver_is_none() {
    // Non-wrapper type — combinator table doesn't apply.
    assert_eq!(combinator_return(&t(), "unwrap"), None);
}

#[test]
fn test_opaque_receiver_is_none() {
    assert_eq!(combinator_return(&CanonicalType::Opaque, "unwrap"), None);
}

#[test]
fn test_slice_receiver_is_none() {
    let slice = CanonicalType::Slice(Box::new(t()));
    assert_eq!(combinator_return(&slice, "iter"), None);
}

// ── End-to-end: chain via Result combinator ──────────────────────

#[test]
fn test_result_chain_unwrap_then_field() {
    // Verifies that combinator lookup produces a `Path` the next layer
    // of inference can index. This is the rlm-bug unblocking pattern.
    use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::{
        infer_type, FlatBindings, InferContext, WorkspaceTypeIndex,
    };
    use std::collections::{HashMap, HashSet};

    let mut index = WorkspaceTypeIndex::new();
    index.struct_fields.insert(
        ("crate::app::Session".to_string(), "id".to_string()),
        CanonicalType::path(["crate", "app", "Id"]),
    );
    let mut bindings = FlatBindings::new();
    bindings.insert(
        "res",
        CanonicalType::Result(Box::new(CanonicalType::path(["crate", "app", "Session"]))),
    );
    let alias_map = HashMap::new();
    let local_symbols = HashSet::new();
    let crate_roots = HashSet::new();
    let ctx = InferContext {
        workspace: &index,
        alias_map: &alias_map,
        local_symbols: &local_symbols,
        crate_root_modules: &crate_roots,
        importing_file: "src/app/test.rs",
        bindings: &bindings,
        self_type: None,
    };
    let expr: syn::Expr = syn::parse_str("res.unwrap().id").expect("parse");
    let t = infer_type(&expr, &ctx).expect("chain resolved");
    assert_eq!(t, CanonicalType::path(["crate", "app", "Id"]));
}
