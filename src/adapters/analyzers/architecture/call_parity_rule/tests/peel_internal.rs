//! Unit tests for the type-peeling helpers: `bindings::strip_wrappers`
//! (parenthesis + stdlib-ownership peel) and
//! `pub_fns_visibility::peel_to_inner_path` (reference/paren arms +
//! `is_transparent_wrapper`'s single-segment `&&`).

use crate::adapters::analyzers::architecture::call_parity_rule::bindings::strip_wrappers;
use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
use crate::adapters::analyzers::architecture::call_parity_rule::pub_fns_visibility::peel_to_inner_path;
use crate::adapters::shared::use_tree::{AliasMap, ScopedAliasMap};
use std::collections::{HashMap, HashSet};

fn ty(src: &str) -> syn::Type {
    syn::parse_str(src).expect("parse type")
}

fn render(ty: &syn::Type) -> String {
    use quote::ToTokens;
    ty.to_token_stream().to_string()
}

// ── bindings::strip_wrappers ────────────────────────────────────────────

#[test]
fn strip_wrappers_peels_parens_and_stdlib_owners() {
    // `(Box<Inner>)` peels the paren then the `Box` to reach `Inner`. Pins
    // the `Type::Paren` arm against deletion (which would return the
    // parenthesised type unpeeled).
    assert_eq!(render(strip_wrappers(&ty("(Box<Inner>)"))), "Inner");
    // A non-wrapper is returned unchanged (positive control).
    assert_eq!(render(strip_wrappers(&ty("Plain"))), "Plain");
}

// ── pub_fns_visibility::peel_to_inner_path ──────────────────────────────

struct Scope {
    alias_map: AliasMap,
    aliases_per_scope: ScopedAliasMap,
    local_symbols: HashSet<String>,
    local_decl_scopes: HashMap<String, Vec<Vec<String>>>,
    crate_roots: HashSet<String>,
}

impl Scope {
    fn new() -> Self {
        Self {
            alias_map: HashMap::new(),
            aliases_per_scope: ScopedAliasMap::new(),
            local_symbols: HashSet::new(),
            local_decl_scopes: HashMap::new(),
            crate_roots: HashSet::new(),
        }
    }

    fn file_scope(&self) -> FileScope<'_> {
        FileScope {
            path: "src/app/x.rs",
            alias_map: &self.alias_map,
            aliases_per_scope: &self.aliases_per_scope,
            local_symbols: &self.local_symbols,
            local_decl_scopes: &self.local_decl_scopes,
            crate_root_modules: &self.crate_roots,
            workspace_module_paths: None,
        }
    }
}

fn peel_leaf(src: &str) -> Option<String> {
    let scope = Scope::new();
    let wraps = HashSet::new();
    peel_to_inner_path(&ty(src), &wraps, &scope.file_scope(), &[]).map(|tp| {
        tp.path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default()
    })
}

#[test]
fn peel_to_inner_path_unwraps_references_and_parens() {
    // `&Inner` and `(Inner)` peel to the inner `Inner` path. Pins the
    // `Type::Reference` and `Type::Paren` arms against deletion (which
    // would drop to the `_ => None` fallback).
    assert_eq!(peel_leaf("&Inner").as_deref(), Some("Inner"), "reference");
    assert_eq!(peel_leaf("(Inner)").as_deref(), Some("Inner"), "paren");
}

#[test]
fn peel_to_inner_path_peels_stdlib_wrapper_but_not_other_generics() {
    // `Box<Inner>` is a transparent stdlib owner → peels to `Inner`;
    // `Mutex<Inner>` is NOT transparent → stays `Mutex`. Pins
    // `is_transparent_wrapper`'s `single && is_stdlib_direct` against `||`
    // (which would peel `Mutex` too).
    assert_eq!(
        peel_leaf("Box<Inner>").as_deref(),
        Some("Inner"),
        "Box peeled"
    );
    assert_eq!(
        peel_leaf("Mutex<Inner>").as_deref(),
        Some("Mutex"),
        "non-transparent wrapper kept"
    );
    // Leading-colon paths bypass the canonicaliser (extern-root gate) and
    // hit the `single && is_stdlib_direct` fallback: `::Box` peels,
    // `::Mutex` does not. Pins that `&&` (line 317) against `||`.
    assert_eq!(
        peel_leaf("::Box<Inner>").as_deref(),
        Some("Inner"),
        "::Box peeled via single-segment fallback"
    );
    assert_eq!(
        peel_leaf("::Mutex<Inner>").as_deref(),
        Some("Mutex"),
        "::Mutex not peeled (single && is_stdlib_direct)"
    );
    // Explicit-stdlib multi-segment paths hit the `explicit_stdlib &&
    // is_stdlib_direct` branch: `std::boxed::Box` peels, `std::sync::Mutex`
    // does not. Pins that `&&` against `||` (which would peel `Mutex`).
    assert_eq!(
        peel_leaf("std::boxed::Box<Inner>").as_deref(),
        Some("Inner"),
        "explicit-std Box peeled"
    );
    assert_eq!(
        peel_leaf("std::sync::Mutex<Inner>").as_deref(),
        Some("Mutex"),
        "explicit-std Mutex not peeled (explicit_stdlib && is_stdlib_direct)"
    );
}
