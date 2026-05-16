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

// qual:api
/// Canonical target an in-scope alias resolves to, plus the bit that
/// distinguishes Rust 2018+ absolute paths (`use ::ext::Foo as Bar;`)
/// from relative ones (`use ext::Foo as Bar;`).
///
/// The absolute-root bit must propagate end-to-end: a leading-colon
/// import names the extern crate `ext`, NOT the workspace's
/// `crate::ext`. Use sites of the alias have to re-apply that gate
/// so workspace canonicalisation doesn't silently prepend `crate::`
/// to an extern-rooted target. Dropping the flag here was the source
/// of the alias-map leading-colon drift fixed in v1.2.4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasTarget {
    pub segments: Vec<String>,
    /// True iff the originating `use` had a leading `::` (extern-root).
    pub absolute_root: bool,
}

impl AliasTarget {
    /// Convenience: a relative-rooted alias target. Test fixtures and
    /// callers that build alias maps inline use this; the parser-side
    /// `collect_alias_entries` constructs them with the right
    /// `absolute_root` from the enclosing `ItemUse`.
    pub fn relative(segments: Vec<String>) -> Self {
        Self {
            segments,
            absolute_root: false,
        }
    }

    pub fn segments(&self) -> &[String] {
        &self.segments
    }
}

/// `name → AliasTarget` — flat alias map for one scope.
pub type AliasMap = HashMap<String, AliasTarget>;

/// Per-mod alias maps within a single file. Key is the mod-path inside
/// the file (empty `Vec` for top-level); value is that mod's own `use`
/// items. Inner mods don't inherit outer entries — Rust requires each
/// mod to re-import names it wants to reference.
pub type ScopedAliasMap = HashMap<Vec<String>, AliasMap>;

/// Apply `f` to every `use` item in the file along with its
/// leading-colon bit. Shared iteration backbone for `gather_imports`
/// / `gather_alias_map` so the two walkers don't duplicate the
/// item-filter — and so callers that need the absolute-root flag get
/// it from the SAME place that exposes the tree, eliminating drift
/// between "did I extract the tree?" and "did I also remember the
/// leading colon?".
/// Operation: iterator-chain dispatch, no own calls.
fn for_each_use_tree<F: FnMut(&UseTree, bool)>(ast: &syn::File, mut f: F) {
    ast.items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Use(u) => Some(u),
            _ => None,
        })
        .for_each(|u| f(&u.tree, u.leading_colon.is_some()));
}

/// Flatten every `use` item in `ast` into its leaf paths.
/// Integration: delegates item iteration + per-tree walk.
pub fn gather_imports(ast: &syn::File) -> Vec<(Vec<String>, proc_macro2::Span)> {
    let mut out = Vec::new();
    for_each_use_tree(ast, |tree, _absolute_root| {
        collect_use_paths(&[], tree, &mut out)
    });
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
/// Build a map from in-scope identifier to its canonical path
/// (segments + absolute-root flag).
///
/// For each `use` leaf the key is the name visible in the file and the
/// value is the full path list, paired with the leading-colon bit of
/// the enclosing `use` item:
/// - `use foo::bar;` → `"bar" → ([foo, bar], absolute_root=false)`.
/// - `use ::foo::bar;` → `"bar" → ([foo, bar], absolute_root=true)`.
/// - `use foo::bar as baz;` → `"baz" → ([foo, bar], absolute_root=…)`.
/// - `use foo::{self, bar};` → `"foo" → ([foo], …)`, `"bar" → ([foo, bar], …)`.
/// - `use foo::*;` is skipped — no bindable identifier.
///
/// Integration: delegates item iteration + per-tree walk.
pub fn gather_alias_map(ast: &syn::File) -> AliasMap {
    let mut out = AliasMap::new();
    for_each_use_tree(ast, |tree, absolute_root| {
        collect_alias_entries(&[], tree, absolute_root, &mut out)
    });
    out
}

// qual:api
/// Like `gather_alias_map`, but separates `use` items by their declaring
/// inline-mod scope. Returns `mod_path → name → AliasTarget`. The
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
                collect_alias_entries(&[], &u.tree, u.leading_colon.is_some(), scope_map);
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
    absolute_root: bool,
    out: &mut AliasMap,
) {
    let store = |out: &mut AliasMap, key: String, segments: Vec<String>| {
        out.insert(
            key,
            AliasTarget {
                segments,
                absolute_root,
            },
        );
    };
    match tree {
        UseTree::Path(p) => {
            let mut next = prefix.to_vec();
            next.push(p.ident.to_string());
            collect_alias_entries(&next, &p.tree, absolute_root, out);
        }
        UseTree::Name(n) => {
            let ident = n.ident.to_string();
            if ident == "self" {
                if let Some(last) = prefix.last().cloned() {
                    store(out, last, prefix.to_vec());
                }
            } else {
                let mut full = prefix.to_vec();
                full.push(ident.clone());
                store(out, ident, full);
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
                    store(out, r.rename.to_string(), vec!["self".to_string()]);
                } else {
                    store(out, r.rename.to_string(), prefix.to_vec());
                }
            } else {
                let mut full = prefix.to_vec();
                full.push(r.ident.to_string());
                store(out, r.rename.to_string(), full);
            }
        }
        UseTree::Glob(_) => {
            // No bindable identifier introduced — skip.
        }
        UseTree::Group(g) => {
            for sub in &g.items {
                collect_alias_entries(prefix, sub, absolute_root, out);
            }
        }
    }
}
