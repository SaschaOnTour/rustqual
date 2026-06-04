//! Unit tests for `reexports.rs` internals: the `pub`-visibility gate
//! (`is_pub_use`), the use-tree leaf collector's `self` handling
//! (`collect_pub_use_leaves`), and the chain-flattening loop bounds
//! (`flatten_chains`).

use crate::adapters::analyzers::architecture::call_parity_rule::reexports::{
    collect_pub_use_leaves, flatten_chains, is_pub_use, ReexportMap,
};

// ── is_pub_use ──────────────────────────────────────────────────────────

fn vis_of(src: &str) -> syn::Visibility {
    let item: syn::ItemUse = syn::parse_str(src).expect("parse use");
    item.vis
}

#[test]
fn is_pub_use_true_only_for_non_inherited_visibility() {
    // `pub use` registers a re-export; bare `use` (Inherited) does not.
    // Pins `is_pub_use -> true` (which would register private imports).
    assert!(is_pub_use(&vis_of("pub use a::B;")), "pub → reexport");
    assert!(
        !is_pub_use(&vis_of("use a::B;")),
        "private use → not a reexport"
    );
}

// ── collect_pub_use_leaves ──────────────────────────────────────────────

fn leaves(src: &str) -> Vec<(Vec<String>, String)> {
    let item: syn::ItemUse = syn::parse_str(src).expect("parse use");
    let mut out = Vec::new();
    collect_pub_use_leaves(&[], &item.tree, &mut out);
    out
}

#[test]
fn collect_leaves_maps_self_to_the_enclosing_module_name() {
    // `use a::b::{self}` re-exports module `b` under the name `b`, NOT
    // `self`. Pins the `ident == "self"` check against `!=` (which would
    // emit a `self` leaf and append `self` to the path).
    let out = leaves("pub use a::b::{self};");
    assert_eq!(out.len(), 1, "one leaf: {out:?}");
    assert_eq!(out[0].1, "b", "self leaf named after the module");
    assert_eq!(out[0].0, vec!["a".to_string(), "b".to_string()], "{out:?}");
    // A normal name is unaffected.
    let normal = leaves("pub use a::C;");
    assert_eq!(normal[0].1, "C");
    // `self as Bar` (the Rename arm) re-exports module `b` under `Bar`
    // with the path staying `a::b` (NOT `a::b::self`). Pins the Rename
    // arm's `r.ident == "self"` check against `!=`.
    let renamed = leaves("pub use a::b::{self as Bar};");
    assert_eq!(renamed.len(), 1, "one leaf: {renamed:?}");
    assert_eq!(renamed[0].1, "Bar", "renamed to Bar");
    assert_eq!(
        renamed[0].0,
        vec!["a".to_string(), "b".to_string()],
        "self-rename keeps the module path: {renamed:?}"
    );
}

// ── flatten_chains ──────────────────────────────────────────────────────

#[test]
fn flatten_chains_resolves_a_terminal_chain_to_its_endpoint() {
    // n0→n1→n2→"end". Every key flattens to the terminal `end`. Pins
    // `flatten_chains -> ()` (would leave n0→n1), the `depth < MAX` start
    // (`==`/`>` break immediately → n0→n1), the `next != current` guard
    // (`==`/false → break immediately), and `depth += 1` → `-=` (usize
    // underflow panic on the first advancing iteration). The `<=`/`*=`
    // mutants only alter the cyclic-input cap, which acyclic real chains
    // never reach — see the docs equivalents note.
    let mut map: ReexportMap = [("n0", "n1"), ("n1", "n2"), ("n2", "end")]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    flatten_chains(&mut map);
    assert_eq!(map["n0"], "end", "n0 flattened to terminal: {map:?}");
    assert_eq!(map["n1"], "end", "n1 flattened to terminal: {map:?}");
}
