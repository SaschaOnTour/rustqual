//! Per-file local-symbol collection with mod-scope awareness.
//!
//! Two views are kept in sync:
//!
//! - `flat: HashSet<String>` — every name declared anywhere in the
//!   file, top-level or nested. Existing callers (`canonicalise_type
//!   _segments`, `local_symbols.contains(name)`) keep working unchanged.
//! - `by_name: HashMap<String, Vec<Vec<String>>>` — per-name list of
//!   mod-paths-within-file where the name is declared. Lets the
//!   scope-aware canonicaliser pick the closest enclosing declaration
//!   so `Session` referenced from inside `mod inner` resolves to
//!   `crate::<file>::inner::Session` when the type is declared there.

use crate::adapters::shared::cfg_test::has_cfg_test;
use std::collections::{HashMap, HashSet};

/// `(flat-set, per-scope-map)` view over the names declared in a file.
#[derive(Debug, Default, Clone)]
pub(crate) struct LocalSymbols {
    pub flat: HashSet<String>,
    pub by_name: HashMap<String, Vec<Vec<String>>>,
}

// qual:api
/// Flat top-level + nested name set. Backward-compatible shape for
/// callers that don't track mod scope. Operation: project flat view.
pub(crate) fn collect_local_symbols(ast: &syn::File) -> HashSet<String> {
    let scoped = collect_local_symbols_scoped(ast);
    scoped.flat
}

// qual:api
/// Scoped variant. Returns both views in one walk so the `flat` set
/// and the `by_name` map are always consistent. Operation.
pub(crate) fn collect_local_symbols_scoped(ast: &syn::File) -> LocalSymbols {
    let mut symbols = LocalSymbols::default();
    walk_local_symbols(&ast.items, &mut Vec::new(), &mut symbols);
    symbols
}

/// Recursive AST walk that populates `LocalSymbols.flat` + `by_name`.
/// `mod_stack` carries the current mod-scope (outer-most first).
/// Operation. Own calls hidden in closure for IOSP leniency.
// qual:recursive
fn walk_local_symbols(items: &[syn::Item], mod_stack: &mut Vec<String>, out: &mut LocalSymbols) {
    let recurse = |inner: &[syn::Item], stack: &mut Vec<String>, out: &mut LocalSymbols| {
        walk_local_symbols(inner, stack, out);
    };
    for item in items {
        if let Some(name) = item_name(item) {
            out.flat.insert(name.clone());
            out.by_name.entry(name).or_default().push(mod_stack.clone());
        }
        if let syn::Item::Mod(m) = item {
            if !has_cfg_test(&m.attrs) {
                if let Some((_, inner)) = m.content.as_ref() {
                    mod_stack.push(m.ident.to_string());
                    recurse(inner, mod_stack, out);
                    mod_stack.pop();
                }
            }
        }
    }
}

// qual:api
/// Walk `mod_stack` outward and return the closest enclosing mod-path
/// in which `name` is declared. Falls back to the empty (top-level)
/// scope when `decl_scopes` is `None` (legacy callers) or the name
/// isn't declared anywhere mappable. Shared by the bindings
/// canonicaliser and the call collector so call canonicals and
/// type-index keys agree on which mod a same-name item belongs to.
/// Operation.
pub(crate) fn scope_for_local<'a>(
    decl_scopes: Option<&'a HashMap<String, Vec<Vec<String>>>>,
    name: &str,
    mod_stack: &[String],
) -> &'a [String] {
    let Some(scopes) = decl_scopes else {
        return &[];
    };
    let Some(candidates) = scopes.get(name) else {
        return &[];
    };
    for depth in (0..=mod_stack.len()).rev() {
        let prefix = &mod_stack[..depth];
        for path in candidates {
            if path.as_slice() == prefix {
                return path.as_slice();
            }
        }
    }
    candidates.first().map(Vec::as_slice).unwrap_or(&[])
}

/// Extract the declared ident from an `Item` if it has one
/// `local_symbols` cares about. Operation: lookup table.
fn item_name(item: &syn::Item) -> Option<String> {
    match item {
        syn::Item::Fn(f) => Some(f.sig.ident.to_string()),
        syn::Item::Mod(m) => Some(m.ident.to_string()),
        syn::Item::Struct(s) => Some(s.ident.to_string()),
        syn::Item::Enum(e) => Some(e.ident.to_string()),
        syn::Item::Union(u) => Some(u.ident.to_string()),
        syn::Item::Trait(t) => Some(t.ident.to_string()),
        syn::Item::Type(t) => Some(t.ident.to_string()),
        syn::Item::Const(c) => Some(c.ident.to_string()),
        syn::Item::Static(s) => Some(s.ident.to_string()),
        _ => None,
    }
}
