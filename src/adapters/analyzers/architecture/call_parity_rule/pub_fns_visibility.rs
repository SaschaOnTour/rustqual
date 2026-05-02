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
use super::pub_fns_alias_chain::{
    chase_alias_chain, collect_alias_chain, resolve_alias_target_canonical,
};
use super::type_infer::resolve::is_stdlib_prefixed;
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

/// Workspace-wide context shared across both passes (alias-chain
/// pre-pass and visible-canonicals collection). Bundles the user
/// transparent-wrapper set and the alias-chain map so functions that
/// need both don't end up with sprawling parameter lists.
struct WalkCtx<'a> {
    transparent_wrappers: &'a HashSet<String>,
    alias_chain: &'a HashMap<String, String>,
}

// qual:api
/// Collect every publicly named type's canonical path across the
/// whole non-test workspace. Integration: pre-builds the
/// alias-chain map, then per-file delegates to the recursive
/// collector.
pub(super) fn collect_visible_type_canonicals_workspace(
    files: &[(&str, &syn::File)],
    cfg_test_files: &HashSet<String>,
    aliases_per_file: &HashMap<String, HashMap<String, Vec<String>>>,
    crate_root_modules: &HashSet<String>,
    transparent_wrappers: &HashSet<String>,
) -> HashSet<String> {
    let alias_chain = collect_alias_chain(
        files,
        cfg_test_files,
        aliases_per_file,
        crate_root_modules,
        transparent_wrappers,
    );
    let ctx = WalkCtx {
        transparent_wrappers,
        alias_chain: &alias_chain,
    };
    let mut out = HashSet::new();
    for_each_file_scope(
        files,
        cfg_test_files,
        aliases_per_file,
        crate_root_modules,
        |file_scope, ast| {
            collect_in_items(&ast.items, &[], file_scope, &ctx, &mut out);
        },
    );
    out
}

/// Build a `FileScope` for each non-cfg-test workspace file and call
/// `body` with it plus the AST. Centralises the per-file construction
/// boilerplate the visibility pass and (via `pub_fns_alias_chain`)
/// the alias-chain pre-pass share. Operation.
fn for_each_file_scope<F>(
    files: &[(&str, &syn::File)],
    cfg_test_files: &HashSet<String>,
    aliases_per_file: &HashMap<String, HashMap<String, Vec<String>>>,
    crate_root_modules: &HashSet<String>,
    mut body: F,
) where
    F: FnMut(&FileScope<'_>, &syn::File),
{
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
        body(&file_scope, ast);
    }
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
    ctx: &WalkCtx<'_>,
    out: &mut HashSet<String>,
) {
    let recurse = |inner: &[syn::Item], next: &[String], out: &mut HashSet<String>| {
        collect_in_items(inner, next, file_scope, ctx, out);
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
        register_alias_target(ty, file_scope, mod_stack, ctx, out);
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

/// `pub type Public = private::Hidden;` (or `Box<private::Hidden>`,
/// `Arc<…>`, etc.) — a public alias can expose methods declared on
/// a hidden source type. Resolve the alias's immediate target through
/// the peel-and-canonicalise pipeline, then chase any further chain
/// entries. Operation.
fn register_alias_target(
    ty: &syn::Type,
    file_scope: &FileScope<'_>,
    mod_stack: &[String],
    ctx: &WalkCtx<'_>,
    out: &mut HashSet<String>,
) {
    let Some(immediate) =
        resolve_alias_target_canonical(ty, file_scope, mod_stack, ctx.transparent_wrappers)
    else {
        return;
    };
    out.insert(immediate.clone());
    chase_alias_chain(&immediate, ctx.alias_chain, out);
}

/// Recursively peel transparent wrappers + references to reach the
/// inner `TypePath`. Returns `None` for types we can't reduce
/// (`RwLock`, `Mutex`, `dyn Trait`, tuples, …) — those don't expose
/// inner methods through Deref. `file_scope` + `mod_stack` are
/// threaded so renamed-import wrappers (`use std::sync::Arc as
/// Shared;`) get recognised through the scope-aware alias resolver.
// qual:recursive
pub(super) fn peel_to_inner_path<'a>(
    ty: &'a syn::Type,
    transparent_wrappers: &HashSet<String>,
    file_scope: &FileScope<'_>,
    mod_stack: &[String],
) -> Option<&'a syn::TypePath> {
    let recurse = |inner: &'a syn::Type| {
        peel_to_inner_path(inner, transparent_wrappers, file_scope, mod_stack)
    };
    match ty {
        syn::Type::Reference(r) => recurse(&r.elem),
        syn::Type::Paren(p) => recurse(&p.elem),
        syn::Type::Path(p) => {
            match transparent_wrapper_inner(p, transparent_wrappers, file_scope, mod_stack) {
                Some(inner) => recurse(inner),
                None => Some(p),
            }
        }
        _ => None,
    }
}

/// If `tp` resolves to a Deref-transparent wrapper — either a
/// stdlib one (`Box`/`Arc`/`Rc`/`Cow`) or a user-configured entry in
/// `[architecture.call_parity]::transparent_wrappers` — return its
/// first generic type arg so the caller can peel further. The
/// resolution mirrors `resolve::resolve_path`'s wrapper detection:
///   - Single-segment bare wrapper names match directly.
///   - Explicit stdlib qualification (`std::boxed::Box<T>`) matches.
///   - Aliased / qualified paths run through the scope-aware
///     canonicaliser; auto-peel only if the canonical is
///     stdlib-prefixed or the leaf is in the user-transparent set.
///
/// Multi-segment paths to local types named `Arc`/`Box`/etc. don't
/// peel — only stdlib-rooted forms do. Returns `None` for non-wrapper
/// paths or wrappers without a positional type arg. Operation.
fn transparent_wrapper_inner<'a>(
    tp: &'a syn::TypePath,
    transparent_wrappers: &HashSet<String>,
    file_scope: &FileScope<'_>,
    mod_stack: &[String],
) -> Option<&'a syn::Type> {
    if !is_transparent_wrapper(&tp.path, transparent_wrappers, file_scope, mod_stack) {
        return None;
    }
    let last = tp.path.segments.last()?;
    let syn::PathArguments::AngleBracketed(ab) = &last.arguments else {
        return None;
    };
    ab.args.iter().find_map(|arg| match arg {
        syn::GenericArgument::Type(t) => Some(t),
        _ => None,
    })
}

/// Decide whether `path` should be treated as a transparent wrapper
/// for visibility-pass peeling. Mirrors `resolve_path`'s wrapper
/// detection (canonicalise-first, fallbacks for unresolvable paths)
/// so the visibility set agrees with the receiver-type resolver.
/// Operation.
fn is_transparent_wrapper(
    path: &syn::Path,
    transparent_wrappers: &HashSet<String>,
    file_scope: &FileScope<'_>,
    mod_stack: &[String],
) -> bool {
    const STDLIB_TRANSPARENT: &[&str] = &["Box", "Arc", "Rc", "Cow"];
    let is_user = |name: &str| transparent_wrappers.contains(name);
    let is_stdlib_direct = |name: &str| STDLIB_TRANSPARENT.contains(&name);
    let Some(last) = path.segments.last() else {
        return false;
    };
    let raw_name = last.ident.to_string();
    let scope = CanonScope {
        file: file_scope,
        mod_stack,
    };
    let segs: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    if let Some(canonical) = canonicalise_type_segments_in_scope(&segs, &scope) {
        let Some(last_seg) = canonical.last() else {
            return false;
        };
        let stdlib_match = is_stdlib_prefixed(&canonical) && is_stdlib_direct(last_seg);
        return stdlib_match || is_user(last_seg);
    }
    if is_user(&raw_name) {
        return true;
    }
    let single = path.segments.len() == 1;
    if single && is_stdlib_direct(&raw_name) {
        return true;
    }
    let first_seg = path.segments.first().map(|s| s.ident.to_string());
    let explicit_stdlib = matches!(first_seg.as_deref(), Some("std" | "core" | "alloc"));
    explicit_stdlib && is_stdlib_direct(&raw_name)
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
    let resolve_source = |segs: &[String], out: &mut HashSet<String>| -> bool {
        let scope = CanonScope {
            file: file_scope,
            mod_stack,
        };
        if let Some(canonical) = canonicalise_type_segments_in_scope(segs, &scope) {
            out.insert(canonical.join("::"));
            true
        } else {
            false
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
            // Only register the export-canonical if the source leaf
            // actually resolves to a *type* in the workspace. A value
            // re-export like `pub use internal::helper as Foo;` would
            // otherwise leak `crate::…::Foo` into the visible-types
            // set and inadvertently pull a same-named private type's
            // impl methods into the call-parity surface.
            let leaf = n.ident.to_string();
            prefix.push(leaf.clone());
            let is_type = resolve_source(prefix, out);
            prefix.pop();
            if is_type {
                add_export(&leaf, out);
            }
        }
        syn::UseTree::Rename(r) => {
            prefix.push(r.ident.to_string());
            let is_type = resolve_source(prefix, out);
            prefix.pop();
            if is_type {
                add_export(&r.rename.to_string(), out);
            }
        }
        syn::UseTree::Group(g) => {
            for sub in &g.items {
                recurse(sub, prefix, out);
            }
        }
        syn::UseTree::Glob(_) => {}
    }
}
