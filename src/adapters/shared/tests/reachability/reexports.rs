//! What `pub use` re-exports expose: names, renames, globs and chains — and
//! the private-module cases that must expose nothing.

use super::*;
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

#[test]
fn reexport_from_a_private_inline_module_is_resolved() {
    // `mod hidden { pub fn entry() }` has no file of its own, so a file-only
    // module index drops `pub use hidden::entry` — and the function, hidden
    // behind a private inline chain, never lands in `pub_items` either.
    let parsed = parse(&[(
        "src/lib.rs",
        "mod hidden { pub fn entry() {} }\npub use hidden::entry;",
    )]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("src/lib.rs", "entry"),
        "an inline module's re-exported item is public API"
    );
}

#[test]
fn a_renamed_reexport_chain_reaches_the_declaration() {
    // `pub use hidden::entry as public_entry` changes the name mid-chain;
    // following only the source name breaks the link from lib.rs.
    let parsed = parse(&[
        ("src/lib.rs", "mod facade;\npub use facade::public_entry;"),
        (
            "src/facade.rs",
            "mod hidden;\npub use self::hidden::entry as public_entry;",
        ),
        ("src/facade/hidden.rs", "pub fn entry() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("src/facade/hidden.rs", "entry"),
        "a rename mid-chain must not break the link"
    );
}

#[test]
fn glob_chains_through_a_glob_exposed_facade_are_followed() {
    // The prelude shape: lib globs a private facade, which globs deeper.
    // Stopping at the first hop leaves the real declaration looking private.
    let parsed = parse(&[
        ("src/lib.rs", "mod facade;\npub use facade::*;"),
        ("src/facade.rs", "mod deep;\npub use self::deep::*;"),
        ("src/facade/deep.rs", "pub fn entry() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("src/facade/deep.rs", "entry"),
        "a glob chain must reach the declaring module"
    );
}

#[test]
fn a_crate_root_style_unprefixed_reexport_still_resolves() {
    // Under uniform paths an unprefixed `use shared::entry` inside a nested
    // module may mean the crate-root `shared`, not a local child. Resolving
    // only relative to the current module would miss it.
    let parsed = parse(&[
        ("src/lib.rs", "pub mod api;\nmod shared;"),
        ("src/api.rs", "pub use shared::entry;"),
        ("src/shared.rs", "pub fn entry() {}"),
    ]);
    let reach = compute_external_reach(&parsed);
    assert!(
        reach.is_externally_reachable("src/shared.rs", "entry"),
        "a crate-root-relative unprefixed re-export must resolve"
    );
}
