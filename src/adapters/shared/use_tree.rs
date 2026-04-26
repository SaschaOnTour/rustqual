//! `syn::UseTree` walker shared across analyzer adapters.
//!
//! Every analyzer that reasons about imports (architecture's layer rule,
//! forbidden rule, glob-import matcher; coupling's module graph; DRY's
//! wildcard detector) needs the same traversal: flatten nested `UseTree`
//! groups into leaf paths with spans. Owning that traversal here keeps
//! the semantics consistent — a single fix applies everywhere.

use std::collections::HashMap;
use syn::spanned::Spanned;
use syn::UseTree;

/// `name → canonical_path_segments` — flat alias map for one scope.
pub type AliasMap = HashMap<String, Vec<String>>;

/// Per-mod alias maps within a single file. Key is the mod-path inside
/// the file (empty `Vec` for top-level); value is that mod's own `use`
/// items. Inner mods don't inherit outer entries — Rust requires each
/// mod to re-import names it wants to reference.
pub type ScopedAliasMap = HashMap<Vec<String>, AliasMap>;

/// Apply `f` to the root `UseTree` of every `use` item in the file.
/// Shared iteration backbone for `gather_imports` / `gather_alias_map`
/// so the two walkers don't duplicate the item-filter.
/// Operation: iterator-chain dispatch, no own calls.
fn for_each_use_tree<F: FnMut(&UseTree)>(ast: &syn::File, mut f: F) {
    ast.items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Use(u) => Some(u),
            _ => None,
        })
        .for_each(|u| f(&u.tree));
}

/// Flatten every `use` item in `ast` into its leaf paths.
/// Integration: delegates item iteration + per-tree walk.
pub fn gather_imports(ast: &syn::File) -> Vec<(Vec<String>, proc_macro2::Span)> {
    let mut out = Vec::new();
    for_each_use_tree(ast, |tree| collect_use_paths(&[], tree, &mut out));
    out
}

/// Collect all leaf import paths from a `use` tree. Each entry is the full
/// list of segments leading to the leaf plus the leaf's span.
/// Operation: recursive tree walk (self-call is recursion, not concern-mixing).
// qual:recursive
pub fn collect_use_paths(
    prefix: &[String],
    tree: &UseTree,
    out: &mut Vec<(Vec<String>, proc_macro2::Span)>,
) {
    match tree {
        UseTree::Path(p) => {
            let mut next = prefix.to_vec();
            next.push(p.ident.to_string());
            collect_use_paths(&next, &p.tree, out);
        }
        UseTree::Name(n) => {
            let mut full = prefix.to_vec();
            full.push(n.ident.to_string());
            out.push((full, n.ident.span()));
        }
        UseTree::Rename(r) => {
            let mut full = prefix.to_vec();
            full.push(r.ident.to_string());
            out.push((full, r.ident.span()));
        }
        UseTree::Glob(g) => {
            out.push((prefix.to_vec(), g.span()));
        }
        UseTree::Group(g) => {
            for sub in &g.items {
                collect_use_paths(prefix, sub, out);
            }
        }
    }
}

// qual:api
/// Build a map from in-scope identifier to its canonical path segments.
///
/// For each `use` leaf the key is the name visible in the file and the
/// value is the full path list:
/// - `use foo::bar;` → `"bar" → [foo, bar]`.
/// - `use foo::bar as baz;` → `"baz" → [foo, bar]` (origin not leaked).
/// - `use foo::{self, bar};` → `"foo" → [foo]`, `"bar" → [foo, bar]`.
/// - `use foo::*;` is skipped — no bindable identifier.
///
/// Integration: delegates item iteration + per-tree walk.
pub fn gather_alias_map(ast: &syn::File) -> HashMap<String, Vec<String>> {
    let mut out = HashMap::new();
    for_each_use_tree(ast, |tree| collect_alias_entries(&[], tree, &mut out));
    out
}

// qual:api
/// Like `gather_alias_map`, but separates `use` items by their declaring
/// inline-mod scope. Returns `mod_path → name → canonical_path`. The
/// empty `Vec` key holds top-level `use` items. Each inline `mod inner
/// { use … }` contributes its own `[…inner]` entry. Inner mods do not
/// inherit outer entries — Rust's name-resolution scoping requires
/// each mod to re-import names it wants to use.
pub fn gather_alias_map_scoped(ast: &syn::File) -> ScopedAliasMap {
    let mut out = ScopedAliasMap::new();
    walk_scoped_aliases(&ast.items, &mut Vec::new(), &mut out);
    out
}

// qual:recursive
fn walk_scoped_aliases(items: &[syn::Item], mod_stack: &mut Vec<String>, out: &mut ScopedAliasMap) {
    let walk = |inner: &[syn::Item], stack: &mut Vec<String>, out: &mut ScopedAliasMap| {
        walk_scoped_aliases(inner, stack, out);
    };
    {
        let scope_map = out.entry(mod_stack.clone()).or_default();
        for item in items {
            if let syn::Item::Use(u) = item {
                collect_alias_entries(&[], &u.tree, scope_map);
            }
        }
    }
    for item in items {
        if let syn::Item::Mod(m) = item {
            if let Some((_, inner)) = m.content.as_ref() {
                mod_stack.push(m.ident.to_string());
                walk(inner, mod_stack, out);
                mod_stack.pop();
            }
        }
    }
}

// qual:recursive
fn collect_alias_entries(
    prefix: &[String],
    tree: &UseTree,
    out: &mut HashMap<String, Vec<String>>,
) {
    match tree {
        UseTree::Path(p) => {
            let mut next = prefix.to_vec();
            next.push(p.ident.to_string());
            collect_alias_entries(&next, &p.tree, out);
        }
        UseTree::Name(n) => {
            let ident = n.ident.to_string();
            if ident == "self" {
                if let Some(last) = prefix.last().cloned() {
                    out.insert(last, prefix.to_vec());
                }
            } else {
                let mut full = prefix.to_vec();
                full.push(ident.clone());
                out.insert(ident, full);
            }
        }
        UseTree::Rename(r) => {
            // `use foo::{self as bar};` parses as Rename { ident: "self",
            // rename: "bar" } — the canonical path is the parent prefix,
            // not prefix + "self". Otherwise downstream alias resolution
            // produces a bogus `foo::self::…` canonical target.
            //
            // Top-level `use self as bar;` (empty prefix) refers to the
            // current file's module; map the alias to `["self"]` so the
            // downstream normaliser resolves it against the importing
            // file instead of silently dropping it.
            if r.ident == "self" {
                if prefix.is_empty() {
                    out.insert(r.rename.to_string(), vec!["self".to_string()]);
                } else {
                    out.insert(r.rename.to_string(), prefix.to_vec());
                }
            } else {
                let mut full = prefix.to_vec();
                full.push(r.ident.to_string());
                out.insert(r.rename.to_string(), full);
            }
        }
        UseTree::Glob(_) => {
            // No bindable identifier introduced — skip.
        }
        UseTree::Group(g) => {
            for sub in &g.items {
                collect_alias_entries(prefix, sub, out);
            }
        }
    }
}
