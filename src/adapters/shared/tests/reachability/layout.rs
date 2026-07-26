//! Where a `mod` declaration's file lives: `#[path]` bases, the `{dir}/{name}.rs`
//! and `{dir}/{name}/mod.rs` conventions, and which files start a crate tree.
//!
//! Every test here also asserts on a *private* item of the target file. An
//! unresolved `mod` leaves that file unwalked, and an unwalked file counts as
//! unknown — therefore reachable. Without the private-item assertion, "resolved
//! and public" and "never found" look exactly alike.

use super::*;

#[test]
fn nested_directory_modules_resolve_through_mod_rs() {
    let parsed = parse(&[
        ("src/lib.rs", "pub mod a;"),
        ("src/a/mod.rs", "pub mod b;"),
        ("src/a/b.rs", "pub fn deep() {} fn hidden() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(reach.is_externally_reachable("src/a/b.rs", "deep"));
    assert!(!reach.is_externally_reachable("src/a/b.rs", "hidden"));
}

#[test]
fn path_attribute_on_a_private_module_still_resolves_for_reexports() {
    // The module is private, so no public edge is recorded — but a `pub use`
    // can still expose its contents. Recording the alias only for public
    // chains leaves the re-export unable to name the module at all.
    let parsed = parse(&[
        (
            "src/lib.rs",
            "#[path = \"custom.rs\"]\nmod hidden;\npub use hidden::entry;",
        ),
        ("src/custom.rs", "pub fn entry() {} fn hidden() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("src/custom.rs", "entry"),
        "a re-export out of a private #[path] module is public API"
    );
    assert!(
        !reach.is_externally_reachable("src/custom.rs", "hidden"),
        "the file is walked — only the re-exported name escapes the private mod"
    );
}

#[test]
fn path_attribute_binds_within_its_own_package() {
    // Two crates both contain `src/custom.rs`. Matching the attribute value
    // against path suffixes anywhere in the workspace can bind crate B's module
    // to crate A's file, leaving B's real file unreachable — a demand to delete
    // a working marker. The base is the declaring file's own directory.
    let parsed = parse(&[
        ("crates/a/src/lib.rs", "pub fn unrelated() {}"),
        ("crates/a/src/custom.rs", "pub fn from_a() {}"),
        (
            "crates/b/src/lib.rs",
            "#[path = \"custom.rs\"]\npub mod api;",
        ),
        (
            "crates/b/src/custom.rs",
            "pub fn from_b() {} fn hidden() {}",
        ),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("crates/b/src/custom.rs", "from_b"),
        "the #[path] module must bind to its own package's file"
    );
    assert!(!reach.is_externally_reachable("crates/b/src/custom.rs", "hidden"));
}

#[test]
fn path_suffix_matching_respects_segment_boundaries() {
    // `api.rs` must bind `src/api.rs`, never `src/myapi.rs` — a resolver that
    // compares by suffix rather than by joined path binds the wrong file.
    let parsed = parse(&[
        ("src/lib.rs", "#[path = \"api.rs\"]\npub mod api;"),
        ("src/myapi.rs", "pub fn wrong() {}"),
        ("src/api.rs", "pub fn right() {} fn hidden() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("src/api.rs", "right"),
        "the exact segment match wins"
    );
    assert!(!reach.is_externally_reachable("src/api.rs", "hidden"));
}

#[test]
fn path_attribute_may_point_outside_its_own_package() {
    // A crate can pull a file in from anywhere: `#[path = "../../../shared/…"]`.
    // Restricting candidates to the declaring package excludes the real target
    // and leaves genuinely public functions looking unreachable.
    let parsed = parse(&[
        (
            "crates/a/src/lib.rs",
            "#[path = \"../../../shared/src/api.rs\"]\npub mod api;",
        ),
        ("shared/src/api.rs", "pub fn entry() {} fn hidden() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("shared/src/api.rs", "entry"),
        "a cross-package #[path] target is still public API"
    );
    assert!(!reach.is_externally_reachable("shared/src/api.rs", "hidden"));
}

#[test]
fn nested_file_under_src_bin_is_not_a_crate_root() {
    // Cargo's autobinaries are `src/bin/name.rs` and `src/bin/name/main.rs`.
    // `src/bin/tools/helper.rs` is neither, so no crate root claims it — and an
    // unclaimed file must stay *unknown* (reachable). Treating it as a root
    // would walk it as a private tree and call its items unreachable.
    let parsed = parse(&[("src/bin/tools/helper.rs", "pub fn helper() {}")]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("src/bin/tools/helper.rs", "helper"),
        "a file no crate root claims stays unknown, not unreachable"
    );
}

#[test]
fn directory_autobinary_main_is_a_crate_root() {
    // `src/bin/tool/main.rs` IS an autobinary root: its tree is walked, and
    // nothing inside a binary is nameable from outside.
    let parsed = parse(&[
        ("src/bin/tool/main.rs", "mod helper;"),
        ("src/bin/tool/helper.rs", "pub fn helper() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        !reach.is_externally_reachable("src/bin/tool/helper.rs", "helper"),
        "a module of a binary crate has no external consumers"
    );
}

#[test]
fn path_attribute_inside_an_inline_module_binds_under_the_module_directory() {
    // rustc resolves a `#[path]` inside an inline block against that block's
    // module directory: from `src/lib.rs`, `mod outer { … }` opens `src/outer/`.
    let parsed = parse(&[
        (
            "src/lib.rs",
            "pub mod outer { #[path = \"custom.rs\"] pub mod inner; }",
        ),
        ("src/outer/custom.rs", "pub fn entry() {} fn hidden() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(reach.is_externally_reachable("src/outer/custom.rs", "entry"));
    assert!(
        !reach.is_externally_reachable("src/outer/custom.rs", "hidden"),
        "the file must really be walked, not merely unknown"
    );
}

#[test]
fn inline_module_path_attribute_in_a_non_mod_rs_file_opens_the_file_directory() {
    // `src/a.rs` is a non-mod-rs file, so its inline `outer` block lives in
    // `src/a/outer/` — the file's own directory *plus* its stem.
    let parsed = parse(&[
        ("src/lib.rs", "pub mod a;"),
        (
            "src/a.rs",
            "pub mod outer { #[path = \"custom.rs\"] pub mod inner; }",
        ),
        ("src/a/outer/custom.rs", "pub fn entry() {} fn hidden() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(reach.is_externally_reachable("src/a/outer/custom.rs", "entry"));
    assert!(!reach.is_externally_reachable("src/a/outer/custom.rs", "hidden"));
}

#[test]
fn top_level_path_attribute_stays_relative_to_the_declaring_file() {
    // Without an inline block the base is the file's *directory*, not its
    // module directory: `src/custom.rs`, never `src/a/custom.rs`.
    let parsed = parse(&[
        ("src/lib.rs", "pub mod a;"),
        ("src/a.rs", "#[path = \"custom.rs\"]\npub mod c;"),
        ("src/custom.rs", "pub fn entry() {} fn hidden() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(reach.is_externally_reachable("src/custom.rs", "entry"));
    assert!(!reach.is_externally_reachable("src/custom.rs", "hidden"));
}

#[test]
fn conventional_child_of_an_inline_module_resolves_under_its_directory() {
    // `pub mod outer { pub mod inner; }` in `src/lib.rs` means
    // `src/outer/inner.rs` — resolving to `src/inner.rs` would bind a
    // different file entirely.
    let parsed = parse(&[
        ("src/lib.rs", "pub mod outer { pub mod inner; }"),
        ("src/inner.rs", "pub fn wrong() {}"),
        ("src/outer/inner.rs", "pub fn entry() {} fn hidden() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(reach.is_externally_reachable("src/outer/inner.rs", "entry"));
    assert!(!reach.is_externally_reachable("src/outer/inner.rs", "hidden"));
}

#[test]
fn path_attribute_with_parent_segments_is_normalised() {
    // `src/a/../shared/api.rs` is `src/shared/api.rs`; comparing the unresolved
    // string against the known paths never matches.
    let parsed = parse(&[
        ("src/lib.rs", "pub mod a;"),
        (
            "src/a/mod.rs",
            "#[path = \"../shared/api.rs\"]\npub mod api;",
        ),
        ("src/shared/api.rs", "pub fn entry() {} fn hidden() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(reach.is_externally_reachable("src/shared/api.rs", "entry"));
    assert!(!reach.is_externally_reachable("src/shared/api.rs", "hidden"));
}

#[test]
fn a_private_inline_module_hides_the_file_its_path_attribute_pulls_in() {
    // The bias only protects files nothing claims. Once the `mod` resolves,
    // its privacy decides — and a stale `qual:api` below it is reportable.
    let parsed = parse(&[
        (
            "src/lib.rs",
            "mod outer { #[path = \"custom.rs\"] pub mod inner; }",
        ),
        ("src/outer/custom.rs", "pub fn entry() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        !reach.is_externally_reachable("src/outer/custom.rs", "entry"),
        "`mod outer` is private, so everything it pulls in is unreachable"
    );
}
