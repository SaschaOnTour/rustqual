//! External reachability: can an item be named from *outside* its crate?
//!
//! `qual:api` only means something for items an outside consumer can call.
//! An item behind a private `mod` is unreachable no matter how many `pub`
//! keywords it carries, so a `qual:api` there is a category error, not a
//! suppression. These tests pin the derivation (file → module path, `pub mod`
//! chain, inline modules, `pub use` re-exports) and its conservative bias:
//! when in doubt, call it reachable, so the marker is left alone.

use crate::adapters::shared::reachability::compute_external_reach;

fn parse(files: &[(&str, &str)]) -> Vec<(String, String, syn::File)> {
    files
        .iter()
        .map(|(path, src)| {
            (
                path.to_string(),
                src.to_string(),
                syn::parse_file(src).expect("fixture must parse"),
            )
        })
        .collect()
}

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

#[test]
fn workspace_crate_roots_are_detected_per_package() {
    let parsed = parse(&[
        ("crates/port/src/lib.rs", "pub mod api;"),
        ("crates/port/src/api.rs", "pub fn entry() {}"),
        ("crates/adapter/src/lib.rs", "mod guts;"),
        ("crates/adapter/src/guts.rs", "pub fn hidden() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(reach.is_externally_reachable("crates/port/src/api.rs", "entry"));
    assert!(!reach.is_externally_reachable("crates/adapter/src/guts.rs", "hidden"));
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
