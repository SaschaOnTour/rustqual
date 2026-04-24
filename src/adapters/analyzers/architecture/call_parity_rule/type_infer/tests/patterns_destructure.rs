//! Tests for `patterns::extract_bindings`.

use super::support::{parse_pat, TypeInferFixture};
use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::{
    extract_bindings, CanonicalType,
};

fn bindings(
    f: &TypeInferFixture,
    pat_src: &str,
    matched: CanonicalType,
) -> Vec<(String, CanonicalType)> {
    let pat = parse_pat(pat_src);
    extract_bindings(&pat, &matched, &f.ctx())
}

// ── Pat::Ident ───────────────────────────────────────────────────

#[test]
fn test_ident_pattern_binds_full_type() {
    let f = TypeInferFixture::new();
    let b = bindings(&f, "x", CanonicalType::path(["crate", "app", "T"]));
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].0, "x");
    assert_eq!(b[0].1, CanonicalType::path(["crate", "app", "T"]));
}

#[test]
fn test_wildcard_binds_nothing() {
    let f = TypeInferFixture::new();
    assert!(bindings(&f, "_", CanonicalType::path(["crate", "T"])).is_empty());
}

// ── Pat::Type (explicit annotation) ──────────────────────────────

#[test]
fn test_type_annotation_overrides_matched() {
    let mut f = TypeInferFixture::new();
    f.local_symbols.insert("Session".to_string());
    // matched type says something else; annotation wins.
    let b = bindings(&f, "x: Session", CanonicalType::Opaque);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].0, "x");
    assert_eq!(
        b[0].1,
        CanonicalType::path(["crate", "app", "test", "Session"])
    );
}

// ── Pat::Reference ───────────────────────────────────────────────

#[test]
fn test_reference_pattern_passes_type_through() {
    let f = TypeInferFixture::new();
    let b = bindings(&f, "&x", CanonicalType::path(["crate", "app", "T"]));
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].1, CanonicalType::path(["crate", "app", "T"]));
}

#[test]
fn test_mutable_reference_pattern_passes_type_through() {
    let f = TypeInferFixture::new();
    let b = bindings(&f, "&mut x", CanonicalType::path(["crate", "app", "T"]));
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].0, "x");
}

// ── Pat::TupleStruct (Some / Ok / Err) ───────────────────────────

#[test]
fn test_some_pattern_unwraps_option() {
    let f = TypeInferFixture::new();
    let opt = CanonicalType::Option(Box::new(CanonicalType::path(["crate", "app", "T"])));
    let b = bindings(&f, "Some(x)", opt);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].0, "x");
    assert_eq!(b[0].1, CanonicalType::path(["crate", "app", "T"]));
}

#[test]
fn test_ok_pattern_unwraps_result() {
    let f = TypeInferFixture::new();
    let res = CanonicalType::Result(Box::new(CanonicalType::path(["crate", "app", "T"])));
    let b = bindings(&f, "Ok(x)", res);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].0, "x");
    assert_eq!(b[0].1, CanonicalType::path(["crate", "app", "T"]));
}

#[test]
fn test_err_pattern_binds_opaque() {
    let f = TypeInferFixture::new();
    let res = CanonicalType::Result(Box::new(CanonicalType::path(["crate", "app", "T"])));
    let b = bindings(&f, "Err(e)", res);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].0, "e");
    // E-side is erased; binding exists but type is Opaque.
    assert_eq!(b[0].1, CanonicalType::Opaque);
}

#[test]
fn test_unknown_variant_binds_opaque() {
    let f = TypeInferFixture::new();
    let matched = CanonicalType::path(["crate", "app", "MyEnum"]);
    let b = bindings(&f, "MyVariant(x)", matched);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].1, CanonicalType::Opaque);
}

#[test]
fn test_none_pattern_binds_nothing() {
    let f = TypeInferFixture::new();
    let opt = CanonicalType::Option(Box::new(CanonicalType::path(["crate", "app", "T"])));
    assert!(bindings(&f, "None", opt).is_empty());
}

// ── Pat::Struct ──────────────────────────────────────────────────

#[test]
fn test_struct_pattern_binds_field_by_name() {
    let mut f = TypeInferFixture::new();
    f.index.struct_fields.insert(
        ("crate::app::Ctx".to_string(), "session".to_string()),
        CanonicalType::path(["crate", "app", "Session"]),
    );
    let matched = CanonicalType::path(["crate", "app", "Ctx"]);
    let b = bindings(&f, "Ctx { session }", matched);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].0, "session");
    assert_eq!(b[0].1, CanonicalType::path(["crate", "app", "Session"]));
}

#[test]
fn test_struct_pattern_with_aliased_field() {
    let mut f = TypeInferFixture::new();
    f.index.struct_fields.insert(
        ("crate::app::Ctx".to_string(), "session".to_string()),
        CanonicalType::path(["crate", "app", "Session"]),
    );
    let matched = CanonicalType::path(["crate", "app", "Ctx"]);
    let b = bindings(&f, "Ctx { session: s }", matched);
    assert_eq!(b.len(), 1);
    // The alias `s` is bound, not `session`.
    assert_eq!(b[0].0, "s");
    assert_eq!(b[0].1, CanonicalType::path(["crate", "app", "Session"]));
}

#[test]
fn test_struct_pattern_missing_field_binds_opaque() {
    let mut f = TypeInferFixture::new();
    let matched = CanonicalType::path(["crate", "app", "Ctx"]);
    // No entry for "unknown" — binding is still made but with Opaque.
    let b = bindings(&f, "Ctx { unknown }", matched);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].1, CanonicalType::Opaque);
    f.self_type = None; // silence unused mut warning
}

#[test]
fn test_struct_pattern_with_rest() {
    let mut f = TypeInferFixture::new();
    f.index.struct_fields.insert(
        ("crate::app::Ctx".to_string(), "a".to_string()),
        CanonicalType::path(["crate", "app", "A"]),
    );
    let matched = CanonicalType::path(["crate", "app", "Ctx"]);
    let b = bindings(&f, "Ctx { a, .. }", matched);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].0, "a");
}

// ── Pat::Tuple ───────────────────────────────────────────────────

#[test]
fn test_tuple_pattern_yields_opaque_bindings() {
    let f = TypeInferFixture::new();
    // We don't track tuple types, so each element gets Opaque.
    let b = bindings(&f, "(a, b)", CanonicalType::Opaque);
    assert_eq!(b.len(), 2);
    assert_eq!(b[0].0, "a");
    assert_eq!(b[0].1, CanonicalType::Opaque);
    assert_eq!(b[1].0, "b");
}

// ── Pat::Slice ───────────────────────────────────────────────────

#[test]
fn test_slice_pattern_distributes_element_type() {
    let f = TypeInferFixture::new();
    let vec_type = CanonicalType::Slice(Box::new(CanonicalType::path(["crate", "T"])));
    let b = bindings(&f, "[first, second]", vec_type);
    assert_eq!(b.len(), 2);
    assert_eq!(b[0].1, CanonicalType::path(["crate", "T"]));
    assert_eq!(b[1].1, CanonicalType::path(["crate", "T"]));
}

#[test]
fn test_slice_pattern_skips_rest() {
    let f = TypeInferFixture::new();
    let vec_type = CanonicalType::Slice(Box::new(CanonicalType::path(["crate", "T"])));
    let b = bindings(&f, "[first, ..]", vec_type);
    // Only `first` is bound; `..` rest is skipped.
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].0, "first");
}

// ── Pat::Or ──────────────────────────────────────────────────────

#[test]
fn test_or_pattern_uses_first_branch_bindings() {
    let f = TypeInferFixture::new();
    // Conservatively take first branch's bindings.
    let matched = CanonicalType::path(["crate", "T"]);
    let b = bindings(&f, "a | b", matched);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].0, "a");
}

// ── Nested patterns ──────────────────────────────────────────────

#[test]
fn test_nested_some_struct() {
    let mut f = TypeInferFixture::new();
    f.index.struct_fields.insert(
        ("crate::app::Ctx".to_string(), "id".to_string()),
        CanonicalType::path(["crate", "app", "Id"]),
    );
    // matched: Option<Ctx>; pattern unwraps Some to Ctx then binds id.
    let opt = CanonicalType::Option(Box::new(CanonicalType::path(["crate", "app", "Ctx"])));
    let b = bindings(&f, "Some(Ctx { id })", opt);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].0, "id");
    assert_eq!(b[0].1, CanonicalType::path(["crate", "app", "Id"]));
}
