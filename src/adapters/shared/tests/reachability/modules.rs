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
fn a_shared_file_is_walked_once_per_crate() {
    // Two packages pull the same file in at the same logical path. Keyed by
    // file and module path alone, the first traversal claims it and the second
    // never runs — so a file that is private in crate A stays "unreachable"
    // even though crate B publishes it, and a `qual:api` on it is reported
    // stale. The crate root is part of a module's identity, so it belongs in
    // the visit key too.
    let parsed = parse(&[
        (
            "crates/a/src/lib.rs",
            "#[path = \"../../../shared/api.rs\"]\nmod api;",
        ),
        (
            "crates/b/src/lib.rs",
            "#[path = \"../../../shared/api.rs\"]\npub mod api;",
        ),
        ("shared/api.rs", "pub fn entry() {} fn hidden() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("shared/api.rs", "entry"),
        "crate B publishes the file; crate A's private `mod` must not hide it"
    );
    assert!(
        !reach.is_externally_reachable("shared/api.rs", "hidden"),
        "the file is walked, so a private item stays unreachable"
    );
}

#[test]
fn public_types_and_constants_are_reachable_items_too() {
    // `qual:api` is verified against this set, and since DRY-006 it can sit on
    // a type. If only functions were recorded, every legitimately public type
    // would be accused of not being nameable from outside.
    let parsed = parse(&[
        ("src/lib.rs", "pub mod api;"),
        (
            "src/api.rs",
            "pub struct Entry; pub enum Mode { A } pub type Alias = u8; \
             pub const MAX: u8 = 1; pub static NAME: &str = \"x\"; struct Hidden;",
        ),
    ]);
    let reach = compute_external_reach(&parsed);
    for name in ["Entry", "Mode", "Alias", "MAX", "NAME"] {
        assert!(
            reach.is_externally_reachable("src/api.rs", name),
            "{name} is public through a public module"
        );
    }
    assert!(!reach.is_externally_reachable("src/api.rs", "Hidden"));
}

#[test]
fn a_public_type_behind_a_private_module_is_not_reachable() {
    let parsed = parse(&[
        ("src/lib.rs", "mod internal;"),
        ("src/internal.rs", "pub struct Looks;"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(!reach.is_externally_reachable("src/internal.rs", "Looks"));
}

#[test]
fn public_methods_are_reachable_items_too() {
    // A method is an `ImplItem`, not an `Item`, so an item-level walk never sees
    // it — and `qual:api` sits on methods more often than on anything else.
    // Missing them accused every marked method of not being nameable from
    // outside its crate, which is the worst shape a finding can have.
    let parsed = parse(&[
        ("src/lib.rs", "pub mod auth;"),
        (
            "src/auth.rs",
            "pub struct Provider; impl Provider { pub fn resolve(&self) {} fn hidden(&self) {} }",
        ),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("src/auth.rs", "resolve"),
        "a pub method on a pub type in a pub module is nameable from outside"
    );
    assert!(!reach.is_externally_reachable("src/auth.rs", "hidden"));
}

#[test]
fn trait_methods_are_reachable_through_their_trait() {
    // A trait's methods carry no visibility of their own — they are as public
    // as the trait.
    let parsed = parse(&[
        ("src/lib.rs", "pub mod api;"),
        (
            "src/api.rs",
            "pub trait Port { fn run(&self); } trait Sealed { fn inner(&self); }",
        ),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(reach.is_externally_reachable("src/api.rs", "run"));
    assert!(!reach.is_externally_reachable("src/api.rs", "inner"));
}

#[test]
fn methods_of_a_reexported_type_are_reachable() {
    // The common façade shape: a private module, its type re-exported at the
    // crate root. The type is callable from outside, so its methods are too —
    // but the file itself sits in no public `mod` chain, and a method's own
    // name is not what the `pub use` publishes.
    let parsed = parse(&[
        ("src/lib.rs", "mod registry;\npub use registry::Registry;"),
        (
            "src/registry.rs",
            "pub struct Registry; impl Registry { pub fn load() {} fn hidden() {} }",
        ),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(reach.is_externally_reachable("src/registry.rs", "Registry"));
    assert!(
        reach.is_externally_reachable("src/registry.rs", "load"),
        "a method is reachable exactly when its type is"
    );
    assert!(!reach.is_externally_reachable("src/registry.rs", "hidden"));
}

#[test]
fn methods_of_an_unreachable_type_stay_unreachable() {
    // The counterpart: no re-export, private module — the method must stay
    // unreachable or the check stops finding anything.
    let parsed = parse(&[
        ("src/lib.rs", "mod registry;"),
        (
            "src/registry.rs",
            "pub struct Registry; impl Registry { pub fn load() {} }",
        ),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(!reach.is_externally_reachable("src/registry.rs", "load"));
}

#[test]
fn methods_in_a_split_impl_find_their_owner() {
    // An inherent impl often lives in a different file from the type — the
    // façade shape again, one module per concern. Asking whether the owner is
    // reachable *in the impl's file* answers no, because the type is not
    // declared there.
    let parsed = parse(&[
        ("src/lib.rs", "pub mod store;\npub use store::Store;"),
        ("src/store.rs", "mod helpers;\npub struct Store;"),
        (
            "src/store/helpers.rs",
            "use super::Store; impl Store { pub fn with_fault(self) -> Self { self } }",
        ),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("src/store/helpers.rs", "with_fault"),
        "the method's owner is reachable, even though it is declared elsewhere"
    );
}
