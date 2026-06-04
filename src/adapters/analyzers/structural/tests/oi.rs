use crate::adapters::analyzers::structural::collect_metadata;
use crate::adapters::analyzers::structural::oi::*;
use crate::adapters::analyzers::structural::{StructuralWarning, StructuralWarningKind};
use crate::config::StructuralConfig;

fn detect_multi(sources: &[(&str, &str)]) -> Vec<StructuralWarning> {
    super::detect_meta(&super::parse_multi(sources), detect_oi)
}

#[test]
fn orphaned_impl_across_cfg_test_files_excluded() {
    // OI must skip test code: a type + inherent impl split across
    // `#![cfg(test)]` companion files are test fixtures, not a production
    // orphaned impl. (Fixed at the metadata source: test files are not
    // collected into StructuralMetadata.)
    let w = detect_multi(&[
        ("src/a_tests.rs", "#![cfg(test)]\nstruct W;"),
        (
            "src/b_tests.rs",
            "#![cfg(test)]\nimpl W { fn go(&self) {} }",
        ),
    ]);
    assert!(
        w.is_empty(),
        "orphaned impl across #![cfg(test)] files must be excluded: {} warning(s)",
        w.len()
    );
}

#[test]
fn trait_seam_with_cfg_test_impl_does_not_trip_oi() {
    // Sibling check to the SIT fix: OI is location-based (inherent impl in a
    // different top-level module than its type def), it does NOT count trait
    // impls, so the DI/test-seam pattern (trait + one prod impl + a cfg(test)
    // double) has no analog of the SIT denominator-shrink bug. Pin that the
    // exact seam shape produces no OrphanedImpl, whether the double is inline…
    let inline = detect_multi(&[(
        "lib.rs",
        "trait Clock { fn now(&self) -> u64; } \
         struct SystemClock; impl Clock for SystemClock { fn now(&self) -> u64 { 0 } } \
         #[cfg(test)] mod tests { use super::*; \
         struct FixedClock(u64); impl Clock for FixedClock { fn now(&self) -> u64 { self.0 } } }",
    )]);
    assert!(inline.is_empty(), "seam pattern must not trip OI (inline)");
    // …or in a separate #![cfg(test)] companion file.
    let split = detect_multi(&[
        (
            "lib.rs",
            "trait Clock { fn now(&self) -> u64; } \
             struct SystemClock; impl Clock for SystemClock { fn now(&self) -> u64 { 0 } }",
        ),
        (
            "clock_tests.rs",
            "#![cfg(test)]\nstruct FixedClock(u64); \
             impl Clock for FixedClock { fn now(&self) -> u64 { self.0 } }",
        ),
    ]);
    assert!(
        split.is_empty(),
        "seam pattern must not trip OI (split file)"
    );
}

#[test]
fn test_same_file_not_flagged() {
    let w = detect_multi(&[("lib.rs", "struct Foo {} impl Foo { fn bar() {} }")]);
    assert!(w.is_empty());
}

#[test]
fn test_different_module_flagged() {
    let w = detect_multi(&[
        ("types.rs", "pub struct Foo {}"),
        ("other.rs", "impl Foo { fn bar() {} }"),
    ]);
    assert_eq!(w.len(), 1);
    assert!(matches!(
        w[0].kind,
        StructuralWarningKind::OrphanedImpl { .. }
    ));
}

#[test]
fn test_same_module_tree_not_flagged() {
    let w = detect_multi(&[
        ("analyzer/mod.rs", "pub struct Analyzer {}"),
        ("analyzer/helpers.rs", "impl Analyzer { fn helper() {} }"),
    ]);
    assert!(w.is_empty(), "same top-level module should not be flagged");
}

#[test]
fn test_trait_impl_not_flagged() {
    // Trait impls are expected in separate files — collect_metadata only puts
    // inherent impls in inherent_impls, not trait impls
    let w = detect_multi(&[
        (
            "types.rs",
            "pub struct Foo {} pub trait Bar { fn baz(&self); }",
        ),
        ("other.rs", "impl Bar for Foo { fn baz(&self) {} }"),
    ]);
    assert!(w.is_empty());
}

#[test]
fn test_external_type_not_flagged() {
    // Type not defined in any parsed file
    let w = detect_multi(&[("other.rs", "impl ExternalType { fn bar() {} }")]);
    assert!(w.is_empty(), "external type should not be flagged");
}

#[test]
fn test_same_module_backslash_paths_not_flagged() {
    // Windows-style backslash paths: same top-level module "db"
    let w = detect_multi(&[
        ("db\\connection.rs", "pub struct Database {}"),
        (
            "db\\queries\\chunks.rs",
            "impl Database { fn get_chunks() {} }",
        ),
    ]);
    assert!(
        w.is_empty(),
        "Same top-level module with backslash paths should not be flagged, got {:?}",
        w.iter().map(|w| &w.file).collect::<Vec<_>>()
    );
}

#[test]
fn test_disabled_check() {
    let parsed: Vec<(String, String, syn::File)> = vec![
        (
            "a.rs".to_string(),
            "pub struct Foo {}".to_string(),
            syn::parse_file("pub struct Foo {}").expect("test"),
        ),
        (
            "b.rs".to_string(),
            "impl Foo { fn bar() {} }".to_string(),
            syn::parse_file("impl Foo { fn bar() {} }").expect("test"),
        ),
    ];
    let meta = collect_metadata(&parsed, &std::collections::HashSet::new());
    let config = StructuralConfig {
        check_oi: false,
        ..StructuralConfig::default()
    };
    let mut warnings = Vec::new();
    detect_oi(&mut warnings, &meta, &config);
    assert!(warnings.is_empty());
}
