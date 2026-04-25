//! Workspace-wide canonical-path collection for publicly named types.
//!
//! `pub_fns` consults this set to decide whether an `impl Type { … }`
//! exposes its methods to external callers. Members are
//! `crate::<file_modules>::<mod_stack>::<ident>` strings — directly
//! comparable against `resolve_impl_self_type`'s output, so two
//! distinct types sharing a short ident don't collide and re-exports /
//! type-aliases bridge to their source canonicals.

use super::bindings::{canonicalise_type_segments_in_scope, CanonScope};
use super::local_symbols::{collect_local_symbols_scoped, FileScope, LocalSymbols};
use crate::adapters::analyzers::architecture::forbidden_rule::file_to_module_segments;
use crate::adapters::shared::cfg_test::has_cfg_test;
use crate::adapters::shared::use_tree::gather_alias_map_scoped;
use std::collections::{HashMap, HashSet};
use syn::Visibility;

/// Visibility modifier counts as "visible for the check" iff it's
/// `pub`, `pub(crate)`, `pub(super)`, or `pub(in <path>)` for any
/// non-`self` path. `Inherited` and `pub(self)` / `pub(in self)`
/// (which Rust treats as equivalent to inherited visibility) both
/// stay out of scope.
pub(super) fn is_visible(vis: &Visibility) -> bool {
    match vis {
        Visibility::Inherited => false,
        Visibility::Restricted(r) => !is_self_restricted(&r.path),
        _ => true,
    }
}

fn is_self_restricted(path: &syn::Path) -> bool {
    path.leading_colon.is_none() && path.segments.len() == 1 && path.segments[0].ident == "self"
}

// qual:api
/// Collect every publicly named type's canonical path across the
/// whole non-test workspace. Integration: per-file delegate to
/// recursive collector.
pub(super) fn collect_visible_type_canonicals_workspace(
    files: &[(&str, &syn::File)],
    cfg_test_files: &HashSet<String>,
    aliases_per_file: &HashMap<String, HashMap<String, Vec<String>>>,
    crate_root_modules: &HashSet<String>,
) -> HashSet<String> {
    let mut out = HashSet::new();
    let empty_aliases = HashMap::new();
    for (path, ast) in files {
        if cfg_test_files.contains(*path) {
            continue;
        }
        let alias_map = aliases_per_file.get(*path).unwrap_or(&empty_aliases);
        let LocalSymbols { flat, by_name } = collect_local_symbols_scoped(ast);
        let aliases_per_scope = gather_alias_map_scoped(ast);
        let file_scope = FileScope {
            path,
            alias_map,
            aliases_per_scope: &aliases_per_scope,
            local_symbols: &flat,
            local_decl_scopes: &by_name,
            crate_root_modules,
        };
        collect_in_items(&ast.items, &[], &file_scope, &mut out);
    }
    out
}

/// Walk a slice of items, inserting publicly named types' canonical
/// paths and recursing into non-cfg-test, visible inline mods. `pub
/// use` items resolve their leaves through the workspace alias /
/// local-symbol pipeline so re-exported source-canonicals enter the
/// set even when the source module itself is private. Glob re-exports
/// (`pub use foo::*`) are intentionally skipped — without expanding
/// the source module we can't statically tell which idents leak.
/// Operation: closure-hidden recursion through nested `mod` blocks.
// qual:recursive
fn collect_in_items(
    items: &[syn::Item],
    mod_stack: &[String],
    file_scope: &FileScope<'_>,
    out: &mut HashSet<String>,
) {
    let recurse = |inner: &[syn::Item], next: &[String], out: &mut HashSet<String>| {
        collect_in_items(inner, next, file_scope, out);
    };
    let add_decl = |ident: &syn::Ident, out: &mut HashSet<String>| {
        out.insert(canonical_for_decl(
            file_scope.path,
            mod_stack,
            &ident.to_string(),
        ));
    };
    let collect_use = |tree: &syn::UseTree, out: &mut HashSet<String>| {
        walk_use_tree(tree, &mut Vec::new(), file_scope, mod_stack, out);
    };
    let add_alias_target = |ty: &syn::Type, out: &mut HashSet<String>| {
        register_alias_target(ty, file_scope, mod_stack, out);
    };
    for item in items {
        match item {
            syn::Item::Struct(s) if is_visible(&s.vis) => add_decl(&s.ident, out),
            syn::Item::Enum(e) if is_visible(&e.vis) => add_decl(&e.ident, out),
            syn::Item::Union(u) if is_visible(&u.vis) => add_decl(&u.ident, out),
            syn::Item::Trait(t) if is_visible(&t.vis) => add_decl(&t.ident, out),
            syn::Item::Type(t) if is_visible(&t.vis) => {
                add_decl(&t.ident, out);
                add_alias_target(&t.ty, out);
            }
            syn::Item::Use(u) if is_visible(&u.vis) => collect_use(&u.tree, out),
            syn::Item::Mod(m) if is_visible(&m.vis) && !has_cfg_test(&m.attrs) => {
                if let Some((_, inner)) = m.content.as_ref() {
                    let mut next = mod_stack.to_vec();
                    next.push(m.ident.to_string());
                    recurse(inner, &next, out);
                }
            }
            _ => {}
        }
    }
}

/// `pub type Public = private::Hidden;` — a public alias can expose
/// methods declared on a hidden source type. Resolve the alias's
/// target type-path through the workspace canonicaliser and add the
/// resolved canonical so impls keyed on the source type are
/// recognised when callers go through the alias. Non-path targets
/// (`pub type Repo = Arc<Store>;`) don't expose target methods
/// directly (the wrapper is what callers see), so they're skipped.
/// Operation.
fn register_alias_target(
    ty: &syn::Type,
    file_scope: &FileScope<'_>,
    mod_stack: &[String],
    out: &mut HashSet<String>,
) {
    let syn::Type::Path(p) = ty else {
        return;
    };
    let segs: Vec<String> = p
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    let scope = CanonScope {
        file: file_scope,
        mod_stack,
    };
    if let Some(canonical) = canonicalise_type_segments_in_scope(&segs, &scope) {
        out.insert(canonical.join("::"));
    }
}

/// Build `crate::<file_modules>::<mod_stack>::<ident>` joined as a
/// single string — the canonical key both `visible_canonicals` and
/// `resolve_impl_self_type` agree on. Operation: pure string assembly.
pub(super) fn canonical_for_decl(file_path: &str, mod_stack: &[String], ident: &str) -> String {
    let mut segs = vec!["crate".to_string()];
    segs.extend(file_to_module_segments(file_path));
    segs.extend(mod_stack.iter().cloned());
    segs.push(ident.to_string());
    segs.join("::")
}

/// Recursive walk over a `pub use` tree. For each leaf, register
/// *two* canonicals in `out`:
/// - The source-canonical: the leaf's full source path resolved
///   through the workspace alias / local-symbol pipeline. Catches
///   impls written against the original declaration site.
/// - The export-canonical: the current scope plus the exported name
///   (rename target if present). Catches impls written against the
///   re-export path (`impl outer::Hidden` after `pub use
///   self::private::Hidden`).
///
/// Operation: closure-hidden descent into nested `Group`s and
/// `Path`s.
// qual:recursive
fn walk_use_tree(
    tree: &syn::UseTree,
    prefix: &mut Vec<String>,
    file_scope: &FileScope<'_>,
    mod_stack: &[String],
    out: &mut HashSet<String>,
) {
    let recurse = |sub: &syn::UseTree, prefix: &mut Vec<String>, out: &mut HashSet<String>| {
        walk_use_tree(sub, prefix, file_scope, mod_stack, out);
    };
    let resolve_source = |segs: &[String], out: &mut HashSet<String>| {
        let scope = CanonScope {
            file: file_scope,
            mod_stack,
        };
        if let Some(canonical) = canonicalise_type_segments_in_scope(segs, &scope) {
            out.insert(canonical.join("::"));
        }
    };
    let add_export = |exported: &str, out: &mut HashSet<String>| {
        out.insert(canonical_for_decl(file_scope.path, mod_stack, exported));
    };
    match tree {
        syn::UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            recurse(&p.tree, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Name(n) => {
            let leaf = n.ident.to_string();
            prefix.push(leaf.clone());
            resolve_source(prefix, out);
            prefix.pop();
            add_export(&leaf, out);
        }
        syn::UseTree::Rename(r) => {
            prefix.push(r.ident.to_string());
            resolve_source(prefix, out);
            prefix.pop();
            add_export(&r.rename.to_string(), out);
        }
        syn::UseTree::Group(g) => {
            for sub in &g.items {
                recurse(sub, prefix, out);
            }
        }
        syn::UseTree::Glob(_) => {}
    }
}
