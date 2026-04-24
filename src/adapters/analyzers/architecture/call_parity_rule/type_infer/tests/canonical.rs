//! Unit tests for the `CanonicalType` vocabulary.

use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::CanonicalType;

#[test]
fn test_path_constructor_from_string_slices() {
    let t = CanonicalType::path(["crate", "app", "Session"]);
    assert_eq!(
        t,
        CanonicalType::Path(vec![
            "crate".to_string(),
            "app".to_string(),
            "Session".to_string()
        ])
    );
}

#[test]
fn test_path_constructor_from_owned_strings() {
    let t = CanonicalType::path(vec!["crate".to_string(), "foo".to_string()]);
    assert!(matches!(t, CanonicalType::Path(_)));
}

#[test]
fn test_is_opaque_detects_opaque() {
    assert!(CanonicalType::Opaque.is_opaque());
    assert!(!CanonicalType::path(["crate", "X"]).is_opaque());
}

#[test]
fn test_happy_inner_unwraps_result() {
    let inner = CanonicalType::path(["crate", "X"]);
    let wrapped = CanonicalType::Result(Box::new(inner.clone()));
    assert_eq!(wrapped.happy_inner(), Some(&inner));
}

#[test]
fn test_happy_inner_unwraps_option() {
    let inner = CanonicalType::path(["crate", "X"]);
    let wrapped = CanonicalType::Option(Box::new(inner.clone()));
    assert_eq!(wrapped.happy_inner(), Some(&inner));
}

#[test]
fn test_happy_inner_unwraps_future() {
    let inner = CanonicalType::path(["crate", "X"]);
    let wrapped = CanonicalType::Future(Box::new(inner.clone()));
    assert_eq!(wrapped.happy_inner(), Some(&inner));
}

#[test]
fn test_happy_inner_none_on_path() {
    let t = CanonicalType::path(["crate", "X"]);
    assert_eq!(t.happy_inner(), None);
}

#[test]
fn test_happy_inner_none_on_opaque() {
    assert_eq!(CanonicalType::Opaque.happy_inner(), None);
}

#[test]
fn test_happy_inner_none_on_slice() {
    let t = CanonicalType::Slice(Box::new(CanonicalType::path(["T"])));
    assert_eq!(t.happy_inner(), None);
}
