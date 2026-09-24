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

/// `files` forwards and backwards, so a test can assert the verdict is the
/// same both ways — the walk order used to decide which same-named
/// definition won. Two orders suffice: the rule under test is an `any` and a
/// sort, neither of which can see order at all.
fn both_orders<'a>(files: &[(&'a str, &'a str)]) -> Vec<Vec<(&'a str, &'a str)>> {
    let mut forward = files.to_vec();
    let mut orders = vec![forward.clone()];
    forward.reverse();
    orders.push(forward);
    orders
}

#[test]
fn an_impl_next_to_its_own_type_is_not_orphaned_by_a_namesake_elsewhere() {
    // Two top-level types named `Thing`; the impl sits with its own. Keying
    // definitions by bare name let whichever file was read last decide, so
    // the same workspace was clean on one machine and flagged on another.
    let files = [
        ("a/src/lib.rs", "pub enum Thing { A }"),
        (
            "b/src/lib.rs",
            "pub enum Thing { B } impl Thing { pub fn f(&self) {} }",
        ),
    ];
    for order in both_orders(&files) {
        let w = detect_multi(&order);
        assert!(w.is_empty(), "{order:?}: {w:?}");
    }
}

#[test]
fn an_impl_far_from_every_namesake_is_orphaned_in_both_orders() {
    // No definition shares the impl's module, so whichever one it means, the
    // impl is orphaned. Which one it means is not knowable by bare name, so
    // the finding names every candidate rather than guess — the same text in
    // both orders.
    let files = [
        ("src/lib.rs", "mod a;\nmod b;\nmod c;"),
        ("src/a.rs", "pub struct Thing;"),
        ("src/c.rs", "pub struct Thing;"),
        ("src/b.rs", "impl Thing { pub fn f(&self) {} }"),
    ];
    for order in both_orders(&files) {
        let w = detect_multi(&order);
        assert_eq!(w.len(), 1, "{order:?}: {w:?}");
        assert!(
            matches!(&w[0].kind, StructuralWarningKind::OrphanedImpl { defining_file } if defining_file == "src/a.rs or src/c.rs"),
            "{order:?}: {w:?}"
        );
    }
}

#[test]
fn a_test_only_type_does_not_vouch_for_a_production_impl() {
    // `#[cfg(test)] struct Thing` does not exist in a production build, so it
    // cannot be what a production `impl Thing` means. Counting it hid a real
    // orphaned impl behind a test fixture of the same name.
    let w = detect_multi(&[
        ("a.rs", "#[cfg(test)] struct Thing;"),
        ("b.rs", "pub struct Thing;"),
        ("a/impls.rs", "impl Thing { fn f(&self) {} }"),
    ]);
    assert_eq!(w.len(), 1, "{w:?}");
}

#[test]
fn an_impl_through_a_crate_path_is_judged_by_bare_name_known_limit() {
    // `impl crate::b::Thing` in `a/impls.rs` means the `Thing` in `b`, but
    // which file holds module `b`'s `Thing` is a resolution question: a
    // re-export (`pub use crate::a::Thing` inside `b.rs`) or `#[path]` breaks
    // "file `b.rs` = module `b`". Narrowing by the path reported correct
    // impls as orphaned, so the namesake in `a` answers for it — a missed
    // finding until module paths are resolved (#60). The same holds in a
    // workspace, where the top-level directory `b/` may be another crate.
    let cases: [&[(&str, &str)]; 2] = [
        &[
            ("src/a.rs", "pub struct Thing;"),
            ("src/b.rs", "pub struct Thing;"),
            ("src/a/impls.rs", "impl crate::b::Thing { fn f(&self) {} }"),
        ],
        &[
            ("x/src/a.rs", "pub struct Thing;"),
            ("x/src/b.rs", "pub struct Thing;"),
            ("b/src/lib.rs", "pub struct Thing;"),
            (
                "x/src/a/impls.rs",
                "impl crate::b::Thing { fn f(&self) {} }",
            ),
        ],
    ];
    for files in cases {
        let w = detect_multi(files);
        assert!(w.is_empty(), "{files:?}: {w:?}");
    }
}

#[test]
fn a_re_export_does_not_make_an_impl_orphaned() {
    // `b.rs` re-exports `a`'s `Thing` and holds an unrelated `Thing` of its
    // own in a nested module; `impl crate::b::Thing` means `a`'s. Reading
    // the path as "the file `b.rs`" reported it orphaned.
    let w = detect_multi(&[
        ("src/a.rs", "pub struct Thing;"),
        (
            "src/b.rs",
            "pub use crate::a::Thing;\nmod inner { pub struct Thing; }",
        ),
        ("src/a/impls.rs", "impl crate::b::Thing { fn f(&self) {} }"),
    ]);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn an_impl_through_a_crate_root_path_is_not_narrowed_known_limit() {
    // `impl crate::Thing` names the crate root, which OI has no module for;
    // the namesake next to the impl answers for it. A missed finding.
    let w = detect_multi(&[
        ("src/lib.rs", "pub struct Thing;"),
        (
            "src/a.rs",
            "pub struct Thing; impl crate::Thing { fn f(&self) {} }",
        ),
    ]);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn an_impl_in_a_path_mapped_child_module_is_at_home() {
    // `#[path = "types.rs"] mod a` makes `types.rs` module `a`, and its child
    // `impls.rs` module `a::impls`. Type and impl sit in `a`; comparing file
    // names reported the impl as orphaned. OI now asks the module tree.
    let w = detect_multi(&[
        ("src/lib.rs", "#[path = \"types.rs\"] mod a;"),
        (
            "src/types.rs",
            "pub struct Thing;\n#[path = \"impls.rs\"] mod impls;",
        ),
        ("src/impls.rs", "impl super::Thing { fn f(&self) {} }"),
    ]);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_namesake_in_another_crate_is_no_candidate() {
    // An inherent impl can only be for a type of its own crate. With the
    // type declared only in other crates, the impl's type is one the analysis
    // did not see (a macro, a build script) — no verdict, not an orphan.
    let w = detect_multi(&[
        ("a/src/lib.rs", "pub struct Thing;"),
        ("b/src/lib.rs", "impl Thing { pub fn f(&self) {} }"),
    ]);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_file_mounted_twice_is_at_home_in_either_module() {
    // `shared.rs` is module `a` and module `b` at once (`#[path]` twice), so
    // it has two logical places; keeping one picked it by hash order and OI
    // came and went from run to run.
    let files = [
        (
            "src/lib.rs",
            "#[path = \"shared.rs\"] mod a;\n#[path = \"shared.rs\"] mod b;",
        ),
        (
            "src/shared.rs",
            "pub struct Thing;\n#[path = \"impls.rs\"] mod impls;",
        ),
        ("src/impls.rs", "impl super::Thing { fn f(&self) {} }"),
    ];
    for _ in 0..20 {
        let w = detect_multi(&files);
        assert!(w.is_empty(), "{w:?}");
    }
}

#[test]
fn an_impl_is_at_home_wherever_the_module_tree_puts_it() {
    // A file is at home at every logical place the module tree gives it: an
    // inline `mod a` in lib.rs is module `a` for the child file `a/impls.rs`,
    // and a file mounted as `a` and as `b::c` is in `a` and in `b`. Giving
    // each file one place reported correct impls as orphaned.
    let cases: [&[(&str, &str)]; 2] = [
        &[
            ("src/lib.rs", "mod a { pub struct Thing; mod impls; }"),
            ("src/a/impls.rs", "impl super::Thing { fn f(&self) {} }"),
        ],
        &[
            (
                "src/lib.rs",
                "#[path = \"shared.rs\"] mod a;\nmod b { #[path = \"../shared.rs\"] pub mod c; mod impls; }",
            ),
            ("src/shared.rs", "pub struct Thing;"),
            ("src/b/impls.rs", "impl super::c::Thing { fn f(&self) {} }"),
        ],
    ];
    for files in cases {
        let w = detect_multi(files);
        assert!(w.is_empty(), "{files:?}: {w:?}");
    }
}

#[test]
fn a_module_declared_twice_under_exclusive_cfgs_is_at_home_in_both() {
    // `mod a` exists twice, under `cfg(unix)` and `cfg(not(unix))`, each
    // pulling in its own file. The module tree kept the first file per module
    // only, so the verdict flipped with the declaration order.
    for lib in [
        "#[cfg(unix)]\n#[path = \"live.rs\"]\nmod a;\n#[cfg(not(unix))]\n#[path = \"other.rs\"]\nmod a;",
        "#[cfg(not(unix))]\n#[path = \"other.rs\"]\nmod a;\n#[cfg(unix)]\n#[path = \"live.rs\"]\nmod a;",
    ] {
        let w = detect_multi(&[
            ("src/lib.rs", lib),
            ("src/live.rs", "pub struct Thing;\n#[path = \"impls.rs\"] mod impls;"),
            ("src/other.rs", "pub struct Other;"),
            ("src/impls.rs", "impl super::Thing { fn f(&self) {} }"),
        ]);
        assert!(w.is_empty(), "{lib}: {w:?}");
    }
}

#[test]
fn a_raw_identifier_module_resolves_to_its_plain_file_name() {
    // `mod r#type` lives in `type.rs` / `type/`, not `r#type…`; missing it
    // left the child file unplaced and OI compared file names.
    let w = detect_multi(&[
        ("src/lib.rs", "mod r#type { pub struct Thing; mod impls; }"),
        ("src/type/impls.rs", "impl super::Thing { fn f(&self) {} }"),
    ]);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_file_the_module_tree_cannot_place_gets_no_verdict() {
    // `cfg_attr(unix, path = "types.rs")` picks the file conditionally; the
    // walk cannot follow it, so neither the type's file nor the impl's is
    // placed. Falling back to comparing file names reported a correct impl;
    // once the tree knows the crate, an unplaced file means no verdict.
    let w = detect_multi(&[
        (
            "src/lib.rs",
            "#[cfg_attr(unix, path = \"types.rs\")]\nmod a;",
        ),
        (
            "src/types.rs",
            "pub struct Thing;\n#[path = \"impls.rs\"] mod impls;",
        ),
        ("src/impls.rs", "impl super::Thing { fn f(&self) {} }"),
    ]);
    assert!(w.is_empty(), "{w:?}");
}
