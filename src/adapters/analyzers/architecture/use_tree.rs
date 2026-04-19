//! Shared `syn::UseTree` walker used by multiple architecture rules.
//!
//! Each call flattens a `use` tree into a list of leaf imports, where a leaf
//! is any terminal of the tree (`Name`, `Rename`, or `Glob`). Groups are
//! expanded and nested `Path` segments are accumulated into the prefix.

use syn::spanned::Spanned;
use syn::UseTree;

/// Flatten every `use` item in `ast` into its leaf paths.
/// Operation: iterator-chain collection (no own-call recording in lenient mode).
pub(super) fn gather_imports(ast: &syn::File) -> Vec<(Vec<String>, proc_macro2::Span)> {
    let mut out = Vec::new();
    ast.items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Use(u) => Some(u),
            _ => None,
        })
        .for_each(|u| collect_use_paths(&[], &u.tree, &mut out));
    out
}

/// Collect all leaf import paths from a `use` tree. Each entry is the full
/// list of segments leading to the leaf plus the leaf's span.
/// Operation: recursive tree walk (self-call is recursion, not concern-mixing).
// qual:recursive
pub(super) fn collect_use_paths(
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
