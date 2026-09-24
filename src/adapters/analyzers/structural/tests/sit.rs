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
    // The impl is collected (it counts toward the total), but through
    // `super::` it is not *certain* — see
    // `a_single_impl_in_a_child_module_is_not_certain_known_limit`. A second
    // impl next to the trait therefore makes no finding either: two impls.
    let w = detect_from(
        "trait Drawable { fn draw(&self); } struct Square; impl Drawable for Square { fn draw(&self) {} } \
         mod inner { struct Circle; impl super::Drawable for Circle { fn draw(&self) {} } }",
    );
    assert!(
        w.is_empty(),
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

fn detect_files(sources: &[(&str, &str)]) -> Vec<StructuralWarning> {
    super::detect_meta(&super::parse_multi(sources), detect_sit)
}

/// Every workspace in `cases` produces no SIT finding.
fn assert_no_sit(cases: &[&[(&str, &str)]]) {
    for files in cases {
        let w = detect_files(files);
        assert!(w.is_empty(), "{files:?}: {w:?}");
    }
}

#[test]
fn a_trait_name_defined_twice_is_not_judged_by_the_wrong_definition() {
    // Two private `Service` traits; only the first has an implementor. Impls
    // are counted by bare trait name, so which trait an impl belongs to is
    // not knowable, and the last definition read used to take the finding —
    // on a trait that has no implementor at all. Undecidable means no
    // finding; the single-impl `Service` in `a` is then a missed one, the
    // safe direction.
    let files = [
        (
            "a.rs",
            "trait Service { fn run(&self); } struct A; impl Service for A { fn run(&self) {} }",
        ),
        ("b.rs", "trait Service { fn run(&self); }"),
    ];
    for order in [files.to_vec(), files.iter().rev().cloned().collect()] {
        let w = detect_files(&order);
        assert!(w.is_empty(), "{order:?}: {w:?}");
    }
}

#[test]
fn an_impl_of_a_foreign_trait_is_not_counted_for_a_local_namesake() {
    // `impl std::fmt::Display for S` implements the std trait, not the
    // private local `Display` — which has no implementor at all. Counting
    // impls by bare trait name reported the local trait as single-impl. An
    // impl whose trait may be foreign — a path that does not start at
    // `crate`/`self`/`super`, or a bare name a non-local `use` brings in —
    // makes the name undecidable.
    let cases: [&[(&str, &str)]; 2] = [
        &[(
            "lib.rs",
            "mod a { trait Display { fn f(&self); } }\nstruct S;\n\
             impl std::fmt::Display for S { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) } }",
        )],
        &[
            ("a.rs", "trait Display { fn f(&self); }"),
            (
                "b.rs",
                "use std::fmt::Display;\nstruct S;\n\
                 impl Display for S { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) } }",
            ),
        ],
    ];
    assert_no_sit(&cases);
}

#[test]
fn a_test_only_trait_does_not_mask_a_production_single_impl() {
    // `#[cfg(test)] trait Service` does not exist in a production build, so
    // it is no second `Service` for the ambiguity rule to trip over.
    let w = detect_files(&[
        (
            "a.rs",
            "trait Service { fn run(&self); } struct A; impl Service for A { fn run(&self) {} }",
        ),
        ("b.rs", "#[cfg(test)] trait Service { fn run(&self); }"),
    ]);
    assert_eq!(w.len(), 1, "{w:?}");
}

#[test]
fn single_impl_findings_come_out_in_a_fixed_order() {
    // Traits sat in a `HashMap`, so two findings came out in either order
    // from run to run — the same input, a different JSON.
    let source = "trait Beta { fn b(&self); } struct B; impl Beta for B { fn b(&self) {} }\n\
                  trait Alpha { fn a(&self); } struct A; impl Alpha for A { fn a(&self) {} }";
    for _ in 0..20 {
        let names: Vec<String> = detect_from(source).into_iter().map(|w| w.name).collect();
        assert_eq!(names, ["Alpha", "Beta"]);
    }
}

#[test]
fn a_prelude_trait_or_a_later_foreign_glob_is_not_counted_for_a_local_namesake() {
    // `impl Default for S` implements the prelude's `Default` without any
    // import; and a foreign glob counts wherever it sits in a group.
    let cases: [&[(&str, &str)]; 2] = [
        &[
            ("a.rs", "trait Default { fn d(&self); }"),
            (
                "b.rs",
                "struct S;\nimpl Default for S { fn default() -> Self { S } }",
            ),
        ],
        &[
            ("a.rs", "trait Display { fn f(&self); }"),
            (
                "b.rs",
                "mod local {}\nuse {crate::local::*, std::fmt::*};\nstruct S;\n\
                 impl Display for S { fn fmt(&self, f: &mut Formatter) -> Result { Ok(()) } }",
            ),
        ],
    ];
    assert_no_sit(&cases);
}

#[test]
fn a_renamed_foreign_import_brings_in_only_its_alias() {
    // After `use std::fmt::Display as StdDisplay` the bare name `Display` is
    // still the local trait, so its single local impl is a finding.
    let w = detect_files(&[(
        "a.rs",
        "use std::fmt::Display as StdDisplay;\ntrait Display { fn f(&self); }\n\
         struct A;\nimpl Display for A { fn f(&self) {} }",
    )]);
    assert_eq!(w.len(), 1, "{w:?}");
}

#[test]
fn a_local_module_import_reads_as_possibly_foreign_known_limit() {
    // `use local::Service` may name a module of this crate or a crate called
    // `local`; bare names cannot tell. It counts as possibly foreign, so the
    // single-impl `Service` is not reported — a missed finding.
    let w = detect_files(&[(
        "a.rs",
        "mod local { pub trait Service { fn run(&self); } }\nuse local::Service;\n\
         struct A;\nimpl Service for A { fn run(&self) {} }",
    )]);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_foreign_trait_reached_through_a_local_re_export_is_not_counted() {
    // `facade` re-exports `std::fmt::Display`; `impl crate::facade::Display`
    // and a bare `Display` imported through the facade both implement the std
    // trait. A local-looking path is no proof of a local trait: any `use` in
    // the workspace binding the name from another crate makes it undecidable.
    let cases: [&[(&str, &str)]; 2] = [
        &[
            ("facade.rs", "pub use std::fmt::Display;"),
            (
                "lib.rs",
                "mod a { trait Display { fn f(&self); } }\nstruct S;\n\
                 impl crate::facade::Display for S { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) } }",
            ),
        ],
        &[
            ("facade.rs", "pub use std::fmt::Display;"),
            ("a.rs", "trait Display { fn f(&self); }"),
            (
                "b.rs",
                "use crate::facade::Display;\nstruct S;\n\
                 impl Display for S { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) } }",
            ),
        ],
    ];
    assert_no_sit(&cases);
}

#[test]
fn an_auto_trait_impl_is_not_counted_for_a_local_namesake() {
    let w = detect_files(&[
        ("a.rs", "trait Send { fn s(&self); }"),
        ("b.rs", "struct S;\nunsafe impl Send for S {}"),
    ]);
    assert!(w.is_empty(), "{w:?}");
}

/// Ways a foreign trait reaches a bare-looking name — each reported a local,
/// unimplemented trait of that name as single-impl. SIT now counts an impl
/// only where it certainly means the private trait: in the trait's own module
/// subtree, through a bare or `self`/`super` path, not a prelude name, with
/// no `use` or glob in its file that could bring the name in from elsewhere.
const FOREIGN_BY_ANOTHER_ROUTE: &[&[(&str, &str)]] = &[
    // A glob re-export in another file, imported by name.
    &[
        ("src/lib.rs", "mod prelude;\nmod a;\nuse crate::prelude::Display;\nstruct S;\n\
          impl Display for S { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) } }"),
        ("src/prelude.rs", "pub use std::fmt::*;"),
        ("src/a.rs", "trait Display { fn f(&self); }"),
    ],
    // A module re-exported, the trait reached through it.
    &[
        ("src/lib.rs", "mod facade;\nmod a;\nstruct S;\n\
          impl crate::facade::fmt::Display for S { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) } }"),
        ("src/facade.rs", "pub use std::fmt;"),
        ("src/a.rs", "trait Display { fn f(&self); }"),
    ],
    // The 2024 prelude.
    &[
        ("src/lib.rs", "mod a;\nstruct S;\nimpl Future for S { type Output = (); fn poll(self: std::pin::Pin<&mut Self>, _: &mut std::task::Context<'_>) -> std::task::Poll<()> { std::task::Poll::Ready(()) } }"),
        ("src/a.rs", "trait Future { fn f(&self); }"),
    ],
];

#[test]
fn a_foreign_trait_by_any_route_is_not_counted_for_a_local_namesake() {
    assert_no_sit(FOREIGN_BY_ANOTHER_ROUTE);
}

#[test]
fn a_single_impl_next_to_its_trait_is_still_found() {
    // The one shape that is certain: trait and impl in one file, the trait
    // named bare, nothing in the file importing that name from elsewhere.
    let w = detect_files(&[(
        "src/lib.rs",
        "trait Service { fn run(&self); }\nstruct A;\nimpl Service for A { fn run(&self) {} }",
    )]);
    assert_eq!(w.len(), 1, "{w:?}");
}

#[test]
fn an_impl_through_self_super_or_a_cfg_is_not_certain() {
    // `self::facade::Display` reaches a re-exported std trait; `super::` can
    // do the same through a parent's re-export; and an impl under any `cfg`
    // may not exist at all — rustqual evaluates no cfg. Each reported a
    // private trait nobody implements as single-impl.
    let cases: [&[(&str, &str)]; 3] = [
        &[
            ("src/lib.rs", "mod facade;\ntrait Display { fn f(&self); }\nstruct S;\n\
              impl self::facade::Display for S { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) } }"),
            ("src/facade.rs", "pub use std::fmt::Display;"),
        ],
        &[
            ("src/lib.rs", "mod facade;\ntrait Display { fn f(&self); }\nmod inner { use self::super::facade::Display;\nstruct S;\n\
              impl Display for S { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) } } }"),
            ("src/facade.rs", "pub use std::fmt::Display;"),
        ],
        &[(
            "src/lib.rs",
            "trait Service { fn run(&self); }\nstruct S;\n#[cfg(any())]\nimpl Service for S { fn run(&self) {} }",
        )],
    ];
    assert_no_sit(&cases);
}

#[test]
fn a_single_impl_in_a_child_module_is_not_certain_known_limit() {
    // `impl super::Service for A` in a child means the parent's trait here —
    // but `super::` could equally reach a re-export in the parent, so it is
    // not counted. A missed finding until names are resolved (#60).
    let w = detect_files(&[(
        "src/lib.rs",
        "trait Service { fn run(&self); }\nmod inner { struct A; impl super::Service for A { fn run(&self) {} } }",
    )]);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn code_behind_a_file_or_module_cfg_is_not_certain() {
    // rustqual evaluates no cfg: a trait under `#![cfg(any())]`, or in a file
    // a `#[cfg(any())] mod m;` pulls in, may not exist at all.
    let cases: [&[(&str, &str)]; 5] = [
        &[(
            "src/lib.rs",
            "#![cfg(any())]\ntrait Service { fn run(&self); }\nstruct S;\nimpl Service for S { fn run(&self) {} }",
        )],
        &[
            ("src/lib.rs", "#[cfg(any())]\nmod m;"),
            (
                "src/m.rs",
                "trait Service { fn run(&self); }\nstruct S;\nimpl Service for S { fn run(&self) {} }",
            ),
        ],
        // Through `#[path]`, and through a gated inline ancestor: the module
        // tree knows which file a gated `mod` pulls in, a name match did not.
        &[
            ("src/lib.rs", "#[cfg(any())]\n#[path = \"optional.rs\"]\nmod gated;"),
            (
                "src/optional.rs",
                "trait Service { fn run(&self); }\nstruct S;\nimpl Service for S { fn run(&self) {} }",
            ),
        ],
        &[
            ("src/lib.rs", "#[cfg(any())]\nmod r#type;"),
            (
                "src/type.rs",
                "trait Service { fn run(&self); }\nstruct S;\nimpl Service for S { fn run(&self) {} }",
            ),
        ],
        &[
            ("src/lib.rs", "#[cfg(any())]\nmod outer { mod child; }"),
            (
                "src/outer/child.rs",
                "trait Service { fn run(&self); }\nstruct S;\nimpl Service for S { fn run(&self) {} }",
            ),
        ],
    ];
    assert_no_sit(&cases);
}

#[test]
fn a_gated_module_name_in_one_crate_does_not_gate_another_crate() {
    // `#[cfg(any())] mod worker;` in crate `a` says nothing about `b`'s
    // `worker.rs`; matching gated modules by name let it withhold `b`'s
    // finding.
    let w = detect_files(&[
        ("a/src/lib.rs", "#[cfg(any())]\nmod worker;"),
        ("b/src/lib.rs", "mod worker;"),
        (
            "b/src/worker.rs",
            "trait Service { fn run(&self); }\nstruct S;\nimpl Service for S { fn run(&self) {} }",
        ),
    ]);
    assert_eq!(w.len(), 1, "{w:?}");
}
