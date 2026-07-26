//! The module-visibility chain: which files a consumer can reach through
//! `pub mod` links, inline blocks, workspaces, binaries and odd layouts.

use super::*;

#[test]
fn pub_item_in_pub_module_chain_is_reachable() {
    let parsed = parse(&[
        ("src/lib.rs", "pub mod outer;"),
        ("src/outer.rs", "pub fn entry() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(reach.is_externally_reachable("src/outer.rs", "entry"));
}

#[test]
fn private_mod_declaration_makes_everything_below_unreachable() {
    // rustqual's own shape: `mod adapters;` without `pub` — every `pub fn`
    // inside is inert for outside consumers.
    let parsed = parse(&[
        ("src/lib.rs", "mod adapters;"),
        ("src/adapters.rs", "pub fn looks_public() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        !reach.is_externally_reachable("src/adapters.rs", "looks_public"),
        "a `pub fn` behind a private `mod` is not reachable from outside"
    );
}

#[test]
fn non_pub_item_in_pub_module_is_unreachable() {
    let parsed = parse(&[
        ("src/lib.rs", "pub mod outer;"),
        (
            "src/outer.rs",
            "pub(crate) fn internal() {} fn private() {}",
        ),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(!reach.is_externally_reachable("src/outer.rs", "internal"));
    assert!(!reach.is_externally_reachable("src/outer.rs", "private"));
}

#[test]
fn crate_root_items_are_reachable() {
    let parsed = parse(&[("src/lib.rs", "pub fn top() {} fn hidden() {}")]);
    let reach = compute_external_reach(&parsed);
    assert!(reach.is_externally_reachable("src/lib.rs", "top"));
    assert!(!reach.is_externally_reachable("src/lib.rs", "hidden"));
}

#[test]
fn inline_module_visibility_is_part_of_the_chain() {
    let parsed = parse(&[(
        "src/lib.rs",
        "pub mod shown { pub fn a() {} } mod hidden { pub fn b() {} }",
    )]);
    let reach = compute_external_reach(&parsed);
    assert!(reach.is_externally_reachable("src/lib.rs", "a"));
    assert!(
        !reach.is_externally_reachable("src/lib.rs", "b"),
        "inline `mod hidden` is private, so `b` is not reachable"
    );
}

#[test]
fn nested_directory_modules_resolve_through_mod_rs() {
    let parsed = parse(&[
        ("src/lib.rs", "pub mod a;"),
        ("src/a/mod.rs", "pub mod b;"),
        ("src/a/b.rs", "pub fn deep() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(reach.is_externally_reachable("src/a/b.rs", "deep"));
}

#[test]
fn a_private_link_anywhere_in_the_chain_breaks_reachability() {
    let parsed = parse(&[
        ("src/lib.rs", "pub mod a;"),
        ("src/a/mod.rs", "mod b;"),
        ("src/a/b.rs", "pub fn deep() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(!reach.is_externally_reachable("src/a/b.rs", "deep"));
}

#[test]
fn pub_use_reexport_rescues_an_item_from_a_private_module() {
    // The escape hatch real crates use: keep the module private, re-export the
    // item at the root. Treating it as unreachable would produce a false
    // "qual:api does not apply" on genuine public API.
    let parsed = parse(&[
        ("src/lib.rs", "mod internal;\npub use internal::Entry;"),
        ("src/internal.rs", "pub fn Entry() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("src/internal.rs", "Entry"),
        "a `pub use` re-export makes the item nameable from outside"
    );
}

#[test]
fn glob_reexport_rescues_the_whole_module() {
    let parsed = parse(&[
        ("src/lib.rs", "mod internal;\npub use internal::*;"),
        ("src/internal.rs", "pub fn anything() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("src/internal.rs", "anything"),
        "a glob re-export covers every pub item of that module"
    );
}

/// A two-crate workspace: `public` exposes its module, `private` hides its
/// own. The module names are parameters so the same fixture covers both the
/// plain per-package case and the name-collision case.
fn two_crate_workspace(public_mod: &str, private_mod: &str) -> ExternalReach {
    let files = [
        (
            "crates/public/src/lib.rs".to_string(),
            format!("pub mod {public_mod};"),
        ),
        (
            format!("crates/public/src/{public_mod}.rs"),
            "pub fn exposed() {}".to_string(),
        ),
        (
            "crates/private/src/lib.rs".to_string(),
            format!("mod {private_mod};"),
        ),
        (
            format!("crates/private/src/{private_mod}.rs"),
            "pub fn hidden() {}".to_string(),
        ),
    ];
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, s)| (p.as_str(), s.as_str()))
        .collect();
    compute_external_reach(&parse(&refs))
}

#[test]
fn workspace_crates_are_resolved_per_package() {
    // With identical module names, a module index keyed only by the path
    // relative to `src/` lets one crate's file overwrite the other's — the
    // public crate's `pub mod api` could then mark the *private* crate's file
    // reachable, or leave the real API file unreachable and turn a valid
    // marker into a false "never applied". (label, public_mod, private_mod)
    for (label, public_mod, private_mod) in [
        ("distinct module names", "api", "guts"),
        ("same module name in both crates", "api", "api"),
    ] {
        let reach = two_crate_workspace(public_mod, private_mod);
        assert!(
            reach.is_externally_reachable(&format!("crates/public/src/{public_mod}.rs"), "exposed"),
            "case {label}: the public crate's module stays reachable"
        );
        assert!(
            !reach
                .is_externally_reachable(&format!("crates/private/src/{private_mod}.rs"), "hidden"),
            "case {label}: the private crate must not inherit the other's pub mod"
        );
    }
}

#[test]
fn unknown_file_is_treated_as_reachable() {
    // Conservative default: a file the derivation does not understand (odd
    // layout, `[lib] path` override) must not produce a false category error.
    let parsed = parse(&[("weird/place.rs", "pub fn f() {}")]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("weird/place.rs", "f"),
        "unrecognised layout ⇒ assume reachable, never a false finding"
    );
}

#[test]
fn binary_crate_items_are_not_externally_reachable() {
    // Nothing can call into a binary from outside, so `qual:api` never applies.
    let parsed = parse(&[
        ("src/main.rs", "pub mod helpers;"),
        ("src/helpers.rs", "pub fn f() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        !reach.is_externally_reachable("src/helpers.rs", "f"),
        "a binary has no outside consumers"
    );
}

#[test]
fn path_attribute_inside_a_nested_inline_module_resolves() {
    // Rust resolves `#[path]` against the enclosing module's directory, not
    // just the declaring file's. Appending to `src/` alone points at a file
    // that does not exist and leaves real API unreachable.
    let parsed = parse(&[
        (
            "src/lib.rs",
            "pub mod outer {\n    #[path = \"custom.rs\"]\n    pub mod inner;\n}",
        ),
        ("src/outer/custom.rs", "pub fn entry() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("src/outer/custom.rs", "entry"),
        "a #[path] module inside an inline module is still public API"
    );
}

#[test]
fn path_attribute_with_parent_segments_resolves() {
    // `#[path = "../shared/api.rs"]` composes to `src/module/../shared/api.rs`
    // while the file is read as `src/shared/api.rs`; an unnormalised string
    // never matches, so the module looks private.
    let parsed = parse(&[
        ("src/lib.rs", "pub mod module;"),
        (
            "src/module/mod.rs",
            "#[path = \"../shared/api.rs\"]\npub mod api;",
        ),
        ("src/shared/api.rs", "pub fn entry() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("src/shared/api.rs", "entry"),
        "a #[path] with parent segments must still resolve"
    );
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
        ("src/custom.rs", "pub fn entry() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("src/custom.rs", "entry"),
        "a re-export out of a private #[path] module is public API"
    );
}

#[test]
fn path_attribute_binds_within_its_own_package() {
    // Two crates both contain `src/custom.rs`. Taking the first suffix match
    // across the whole workspace can bind crate B's module to crate A's file,
    // leaving B's real file unreachable — a demand to delete a working marker.
    let parsed = parse(&[
        ("crates/a/src/lib.rs", "pub fn unrelated() {}"),
        ("crates/a/src/custom.rs", "pub fn from_a() {}"),
        (
            "crates/b/src/lib.rs",
            "#[path = \"custom.rs\"]\npub mod api;",
        ),
        ("crates/b/src/custom.rs", "pub fn from_b() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("crates/b/src/custom.rs", "from_b"),
        "the #[path] module must bind to its own package's file"
    );
}

#[test]
fn path_suffix_matching_respects_segment_boundaries() {
    // `api.rs` must not match `myapi.rs`; combined with a single-match pick
    // that would silently bind the wrong file.
    let parsed = parse(&[
        ("src/lib.rs", "#[path = \"api.rs\"]\npub mod api;"),
        ("src/myapi.rs", "pub fn wrong() {}"),
        ("src/api.rs", "pub fn right() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("src/api.rs", "right"),
        "the exact segment match wins"
    );
}

#[test]
fn path_attribute_may_point_outside_its_own_package() {
    // A crate can pull a file in from anywhere: `#[path = "../../shared/…"]`.
    // Restricting candidates to the declaring package excludes the real target
    // and leaves genuinely public functions looking unreachable.
    let parsed = parse(&[
        (
            "crates/a/src/lib.rs",
            "#[path = \"../../shared/src/api.rs\"]\npub mod api;",
        ),
        ("shared/src/api.rs", "pub fn entry() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("shared/src/api.rs", "entry"),
        "a cross-package #[path] target is still public API"
    );
}
