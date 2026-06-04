use crate::adapters::analyzers::structural::collect_metadata;
use crate::adapters::analyzers::structural::sit::*;
use crate::adapters::analyzers::structural::{StructuralWarning, StructuralWarningKind};
use crate::config::StructuralConfig;

fn detect_from(source: &str) -> Vec<StructuralWarning> {
    super::detect_meta(&super::parse_single(source), detect_sit)
}

#[test]
fn single_impl_trait_in_cfg_test_file_excluded() {
    // SIT must skip test code: a non-pub trait with one impl inside a
    // `#![cfg(test)]` file is a test mock, not a production over-abstraction.
    let w = detect_from(
        "#![cfg(test)]\ntrait Drawable { fn draw(&self); } struct Circle; impl Drawable for Circle { fn draw(&self) {} }",
    );
    assert!(
        w.is_empty(),
        "single-impl trait in a #![cfg(test)] file must be excluded: {} warning(s)",
        w.len()
    );
}

#[test]
fn test_single_impl_flagged() {
    let w = detect_from(
        "trait Drawable { fn draw(&self); } struct Circle; impl Drawable for Circle { fn draw(&self) {} }",
    );
    assert_eq!(w.len(), 1);
    assert!(matches!(
        w[0].kind,
        StructuralWarningKind::SingleImplTrait { .. }
    ));
    assert_eq!(w[0].name, "Drawable");
}

#[test]
fn test_multiple_impls_not_flagged() {
    let w = detect_from(
        "trait Drawable { fn draw(&self); } struct Circle; struct Square; impl Drawable for Circle { fn draw(&self) {} } impl Drawable for Square { fn draw(&self) {} }",
    );
    assert!(w.is_empty());
}

#[test]
fn test_pub_trait_excluded() {
    let w = detect_from(
        "pub trait Drawable { fn draw(&self); } struct Circle; impl Drawable for Circle { fn draw(&self) {} }",
    );
    assert!(w.is_empty());
}

#[test]
fn test_marker_trait_excluded() {
    let w = detect_from("trait Marker {} struct Circle; impl Marker for Circle {}");
    assert!(w.is_empty());
}

#[test]
fn test_zero_impls_not_flagged() {
    let w = detect_from("trait Drawable { fn draw(&self); }");
    assert!(w.is_empty());
}

#[test]
fn single_impl_in_non_test_module_collected() {
    // The metadata collector must descend into regular (non-test) modules: an
    // impl living in `mod inner` must still count toward the trait's impl set
    // so SIT sees the single implementor. Guards the metadata-recursion guard
    // in `collect_item_metadata` against being skipped.
    let w = detect_from(
        "trait Drawable { fn draw(&self); } mod inner { struct Circle; impl super::Drawable for Circle { fn draw(&self) {} } }",
    );
    assert_eq!(
        w.len(),
        1,
        "an impl inside a non-test module must be collected into the metadata"
    );
}

#[test]
fn test_disabled_check() {
    let syntax =
        syn::parse_file("trait D { fn d(&self); } struct C; impl D for C { fn d(&self) {} }")
            .expect("test source");
    let parsed = vec![("lib.rs".to_string(), String::new(), syntax)];
    let meta = collect_metadata(&parsed, &std::collections::HashSet::new());
    let config = StructuralConfig {
        check_sit: false,
        ..StructuralConfig::default()
    };
    let mut warnings = Vec::new();
    detect_sit(&mut warnings, &meta, &config);
    assert!(warnings.is_empty());
}
