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
fn single_prod_impl_with_inline_cfg_test_impl_is_a_seam_not_sit() {
    // A non-pub trait with ONE production impl plus a `#[cfg(test)]` test-double
    // impl is the idiomatic DI / test-seam pattern, not an over-abstraction: the
    // trait genuinely has multiple implementers and is used polymorphically.
    // SIT must count the cfg(test) impl toward the total and not fire.
    let w = detect_from(
        "trait Clock { fn now(&self) -> u64; } \
         struct SystemClock; impl Clock for SystemClock { fn now(&self) -> u64 { 0 } } \
         #[cfg(test)] mod tests { use super::*; \
         struct FixedClock(u64); impl Clock for FixedClock { fn now(&self) -> u64 { self.0 } } }",
    );
    assert!(
        w.is_empty(),
        "trait with a #[cfg(test)] impl is a test seam, not single-impl: {} warning(s)",
        w.len()
    );
}

#[test]
fn single_prod_impl_with_cfg_test_impl_in_separate_file_not_flagged() {
    // The real-world shape: the trait + its production impl live in lib.rs, the
    // test-double impl in a separate `#![cfg(test)]` companion file (so the file
    // is whole-file test-classified and skipped by metadata collection). The
    // cfg(test) impl must still be counted toward the trait's total.
    let w = super::detect_meta(
        &super::parse_multi(&[
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
        ]),
        detect_sit,
    );
    assert!(
        w.is_empty(),
        "cfg(test) impl in a separate test file must count toward the total: {} warning(s)",
        w.len()
    );
}

#[test]
fn single_prod_impl_with_two_cfg_test_impls_not_flagged() {
    // Mirrors the reported crate exactly: 1 production impl + 2 cfg(test) impls.
    // Multiple test doubles change nothing — total impls > 1, so no SIT.
    let w = detect_from(
        "trait Clock { fn now(&self) -> u64; } \
         struct SystemClock; impl Clock for SystemClock { fn now(&self) -> u64 { 0 } } \
         #[cfg(test)] mod a { use super::*; struct A; impl Clock for A { fn now(&self) -> u64 { 1 } } } \
         #[cfg(test)] mod b { use super::*; struct B; impl Clock for B { fn now(&self) -> u64 { 2 } } }",
    );
    assert!(
        w.is_empty(),
        "1 prod + 2 test impls is not a single-impl trait"
    );
}

#[test]
fn single_prod_impl_zero_test_impls_still_flagged() {
    // Guard the fix against over-broadening: a genuine single-impl trait (one
    // production impl, no test doubles anywhere) must STILL be flagged — that is
    // the real over-abstraction SIT exists to catch.
    let w = detect_from(
        "trait Clock { fn now(&self) -> u64; } \
         struct SystemClock; impl Clock for SystemClock { fn now(&self) -> u64 { 0 } }",
    );
    assert_eq!(w.len(), 1, "1 prod + 0 test impls is still SIT");
    assert_eq!(w[0].name, "Clock");
}

#[test]
fn cfg_test_attr_impl_is_not_a_production_impl() {
    // A `#[cfg(test)]` attribute directly on the impl ITEM (not inside a
    // `#[cfg(test)] mod`, not in a test file) makes it a test impl. A trait
    // whose only implementor is such an impl has ZERO production impls and must
    // not be flagged — previously the item-level attr was ignored and the impl
    // miscounted as a single production impl.
    let w = detect_from(
        "trait Clock { fn now(&self) -> u64; } \
         struct Fake; #[cfg(test)] impl Clock for Fake { fn now(&self) -> u64 { 0 } }",
    );
    assert!(
        w.is_empty(),
        "a #[cfg(test)] impl item is a test impl, not a single production impl: {} warning(s)",
        w.len()
    );
}

#[test]
fn single_prod_impl_with_cfg_test_attr_double_not_flagged() {
    // The DI seam expressed via an item-level `#[cfg(test)]` double: one
    // production impl plus a test double attributed directly on the impl. Total
    // implementors 2 → no SIT.
    let w = detect_from(
        "trait Clock { fn now(&self) -> u64; } \
         struct SystemClock; impl Clock for SystemClock { fn now(&self) -> u64 { 0 } } \
         struct FixedClock; #[cfg(test)] impl Clock for FixedClock { fn now(&self) -> u64 { 1 } }",
    );
    assert!(
        w.is_empty(),
        "1 prod + 1 #[cfg(test)] item-attr double is a seam, not SIT"
    );
}

#[test]
fn real_double_counts_even_with_unrelated_same_named_test_trait() {
    // The critical no-false-positive guarantee: a production trait WITH a real
    // test double in one test module is NOT re-flagged even when an unrelated
    // test-only trait of the same name lives in another test module. Pure
    // name-based counting tallies BOTH "Clock" impls, so the trait's total
    // implementor count exceeds 1 and SIT stays silent — a legitimate DI seam is
    // never re-flagged. (Earlier scope/path heuristics could wrongly drop the
    // real double here.)
    let w = detect_from(
        "trait Clock { fn now(&self) -> u64; } \
         struct SystemClock; impl Clock for SystemClock { fn now(&self) -> u64 { 0 } } \
         #[cfg(test)] mod doubles { use super::*; \
         struct FixedClock; impl Clock for FixedClock { fn now(&self) -> u64 { 1 } } } \
         #[cfg(test)] mod local { \
         trait Clock { fn tick(&self); } struct T; impl Clock for T { fn tick(&self) {} } }",
    );
    assert!(
        w.is_empty(),
        "a real test double must still count even when an unrelated test-only trait shares the name: {} warning(s)",
        w.len()
    );
}

#[test]
fn real_double_via_super_not_reflagged_with_same_named_nested_test_trait() {
    // No-false-positive pin for the nested-`super::` case: a nested test module
    // defines its own `trait Clock` AND impls `super::super::Clock` (a real
    // double of the production trait). Pure name-based counting tallies that impl
    // under "Clock", so the production trait is not re-flagged. (A scope/path
    // heuristic wrongly dropped this double because the nested module defined a
    // same-named trait — re-introducing the very false positive SIT-with-doubles
    // exists to avoid.)
    let w = detect_from(
        "trait Clock { fn now(&self) -> u64; } \
         struct SystemClock; impl Clock for SystemClock { fn now(&self) -> u64 { 0 } } \
         #[cfg(test)] mod tests { mod inner { \
         trait Clock { fn tick(&self); } struct T; \
         impl super::super::Clock for T { fn now(&self) -> u64 { 1 } } } }",
    );
    assert!(
        w.is_empty(),
        "a real super:: double must never be dropped: {} warning(s)",
        w.len()
    );
}

#[test]
fn name_collision_with_test_only_trait_under_reports_sit() {
    // KNOWN, DOCUMENTED LIMITATION (deliberately the SAFE direction): counting
    // is purely name-based, so an unrelated test-only `trait Clock` whose impl
    // shares the production trait's last-segment name is counted toward it,
    // making SIT *under-report* a genuinely single-impl production `Clock`. This
    // is the accepted trade for never producing a false positive (see the two
    // tests above). Precisely distinguishing the two same-named traits would need
    // real trait identity, which the name-keyed metadata models on neither side
    // (the production side has the same collision). Pinned so the trade-off is
    // explicit and any future change to it is deliberate.
    let w = detect_from(
        "trait Clock { fn now(&self) -> u64; } \
         struct SystemClock; impl Clock for SystemClock { fn now(&self) -> u64 { 0 } } \
         #[cfg(test)] mod tests { \
         trait Clock { fn tick(&self); } struct TestClock; impl Clock for TestClock { fn tick(&self) {} } }",
    );
    assert!(
        w.is_empty(),
        "name-based counting under-reports here (documented limitation), got {} warning(s)",
        w.len()
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
