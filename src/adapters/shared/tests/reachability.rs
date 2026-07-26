//! External reachability: can an item be named from *outside* its crate?
//!
//! `qual:api` only means something for items an outside consumer can call.
//! An item behind a private `mod` is unreachable no matter how many `pub`
//! keywords it carries, so a `qual:api` there is a category error, not a
//! suppression. These tests pin the derivation (file → module path, `pub mod`
//! chain, inline modules, `pub use` re-exports) and its conservative bias:
//! when in doubt, call it reachable, so the marker is left alone.

use crate::adapters::shared::reachability::{compute_external_reach, ExternalReach};

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
fn a_reexport_does_not_make_every_same_named_function_reachable() {
    // `pub use public_impl::run` exposes exactly one `run`. Treating the bare
    // name as globally re-exported would silently excuse an invalid marker on
    // an unrelated `run` in a private module.
    let parsed = parse(&[
        (
            "src/lib.rs",
            "mod public_impl;\nmod other;\npub use public_impl::run;",
        ),
        ("src/public_impl.rs", "pub fn run() {}"),
        ("src/other.rs", "pub fn run() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("src/public_impl.rs", "run"),
        "the re-exported one is reachable"
    );
    assert!(
        !reach.is_externally_reachable("src/other.rs", "run"),
        "an unrelated same-named fn must not ride along on the re-export"
    );
}

#[test]
fn super_prefixed_reexports_resolve_to_the_parent_module() {
    // `pub use super::internal::entry` inside `mod api` names the *parent's*
    // `internal`. Appending "super" as a module segment would look for
    // `api::super::internal`, find nothing, and turn a valid marker into a
    // false "never applied".
    let parsed = parse(&[
        ("src/lib.rs", "pub mod api;\nmod internal;"),
        ("src/api.rs", "pub use super::internal::entry;"),
        ("src/internal.rs", "pub fn entry() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("src/internal.rs", "entry"),
        "a `super::`-prefixed re-export must resolve to the parent module"
    );
}

#[test]
fn reexport_chains_are_followed_to_the_declaration() {
    // Façade/prelude modules re-export in steps: lib → facade → hidden. Only
    // resolving one hop leaves the real declaration looking unreachable.
    let parsed = parse(&[
        ("src/lib.rs", "mod facade;\npub use facade::entry;"),
        ("src/facade.rs", "mod hidden;\npub use hidden::entry;"),
        ("src/facade/hidden.rs", "pub fn entry() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("src/facade/hidden.rs", "entry"),
        "a multi-step re-export chain must reach the declaring file"
    );
}

#[test]
fn a_reexport_inside_a_private_module_does_not_expose_anything() {
    // `private_facade.rs` is only reached via `mod private_facade;`, so its
    // `pub use` cannot be named from outside — treating it as an export would
    // silently excuse an invalid marker on the target.
    let parsed = parse(&[
        ("src/lib.rs", "mod private_facade;\nmod target;"),
        ("src/private_facade.rs", "pub use super::target::hidden;"),
        ("src/target.rs", "pub fn hidden() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        !reach.is_externally_reachable("src/target.rs", "hidden"),
        "a re-export from a private module exposes nothing"
    );
}
