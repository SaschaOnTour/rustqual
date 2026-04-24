//! Tests for `patterns::extract_for_bindings`.

use super::support::{parse_pat, TypeInferFixture};
use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::{
    extract_for_bindings, CanonicalType,
};

fn for_bindings(
    f: &TypeInferFixture,
    pat_src: &str,
    iter: CanonicalType,
) -> Vec<(String, CanonicalType)> {
    let pat = parse_pat(pat_src);
    extract_for_bindings(&pat, &iter, &f.ctx())
}

#[test]
fn test_for_over_slice_binds_element_type() {
    let f = TypeInferFixture::new();
    let vec_type = CanonicalType::Slice(Box::new(CanonicalType::path(["crate", "T"])));
    let b = for_bindings(&f, "x", vec_type);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].0, "x");
    assert_eq!(b[0].1, CanonicalType::path(["crate", "T"]));
}

#[test]
fn test_for_over_map_yields_opaque_pair() {
    let f = TypeInferFixture::new();
    let map_type = CanonicalType::Map(Box::new(CanonicalType::path(["crate", "V"])));
    // HashMap yields (&K, &V) — we don't track tuples, so destructuring
    // gives Opaque (though tuple binding shape is preserved).
    let b = for_bindings(&f, "(k, v)", map_type);
    assert_eq!(b.len(), 2);
    assert_eq!(b[0].1, CanonicalType::Opaque);
    assert_eq!(b[1].1, CanonicalType::Opaque);
}

#[test]
fn test_for_over_opaque_binds_opaque() {
    let f = TypeInferFixture::new();
    let b = for_bindings(&f, "item", CanonicalType::Opaque);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].1, CanonicalType::Opaque);
}

#[test]
fn test_for_with_destructuring_pattern() {
    let mut f = TypeInferFixture::new();
    f.index.struct_fields.insert(
        ("crate::app::Handler".to_string(), "id".to_string()),
        CanonicalType::path(["crate", "app", "Id"]),
    );
    let vec_type = CanonicalType::Slice(Box::new(CanonicalType::path(["crate", "app", "Handler"])));
    let b = for_bindings(&f, "Handler { id }", vec_type);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].0, "id");
    assert_eq!(b[0].1, CanonicalType::path(["crate", "app", "Id"]));
}

#[test]
fn test_for_with_wildcard_binds_nothing() {
    let f = TypeInferFixture::new();
    let vec_type = CanonicalType::Slice(Box::new(CanonicalType::path(["crate", "T"])));
    assert!(for_bindings(&f, "_", vec_type).is_empty());
}
