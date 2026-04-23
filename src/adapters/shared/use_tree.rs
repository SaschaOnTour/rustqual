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
            let mut full = prefix.to_vec();
            full.push(r.ident.to_string());
            out.insert(r.rename.to_string(), full);
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
