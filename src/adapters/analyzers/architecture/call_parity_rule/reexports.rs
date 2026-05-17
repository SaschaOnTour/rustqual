//! Workspace-wide `pub use` re-export resolution.
//!
//! Rust's name resolution treats `pub use foo::bar` as a path rewrite:
//! callers writing `this_mod::bar` and callers writing `foo::bar` both
//! refer to the same fn definition. rustqual reads pre-expansion
//! source and canonicalises each call site against its file's local
//! alias map only — without a re-export pass, the call
//! `middleware::record_operation()` (when `middleware/mod.rs` carries
//! `pub use savings_recorder::record_operation`) canonicalises to
//! `crate::application::middleware::record_operation`, which has no
//! fn definition node, and the edge disappears from the call graph.
//!
//! This module collects every workspace `pub use` leaf into a
//! `reexport_map: HashMap<reexport_canonical → target_canonical>`
//! and `rewrite_reexport_edges` applies it as a post-pass on the
//! call graph (chained re-exports are flattened with a depth-bound
//! loop). Inherent `use foo::bar` (private to the importing file)
//! is *not* a re-export and is excluded — the existing per-file
//! alias map already handles those.

use super::bindings::{canonicalise_workspace_path, CanonScope};
use super::local_symbols::FileScope;
use super::workspace_graph::{apply_edge_rewrite, CallGraph};
use crate::adapters::analyzers::architecture::forbidden_rule::file_to_module_segments;
use crate::adapters::shared::cfg_test::has_cfg_test;
use std::collections::HashMap;

/// Hard cap on chained-reexport flattening to prevent pathological
/// cycles (`pub use a::b; pub use b::a` — illegal in Rust but the
/// detector mustn't loop). Real-world chains bottom out under 4.
const MAX_REEXPORT_CHAIN_DEPTH: usize = 16;

/// `reexport_canonical → target_canonical` map across the workspace.
/// Built once by `collect_reexport_map` and read both by the
/// canonicalisation gate (`bindings::canonicalise_workspace_path`)
/// and by the defense-in-depth post-pass `rewrite_reexport_edges`.
pub(crate) type ReexportMap = HashMap<String, String>;

// qual:api
/// Walk every `pub use` (and `pub(crate)`/`pub(super)`/`pub(in path)`
/// use) leaf across the workspace and produce a map from
/// reexport-site canonical → original-definition canonical. Chained
/// re-exports are flattened iteratively.
pub(crate) fn collect_reexport_map(
    files: &[(&str, &syn::File)],
    workspace_files: &HashMap<String, FileScope<'_>>,
) -> ReexportMap {
    let mut map: ReexportMap = HashMap::new();
    for (path, ast) in files {
        let Some(file) = workspace_files.get(*path) else {
            continue;
        };
        walk_pub_uses(&ast.items, &mut Vec::new(), file, &mut map);
    }
    flatten_chains(&mut map);
    map
}

// qual:api
/// Normalise a fully-resolved workspace canonical through the
/// re-export map: if the canonical (or a strict `::`-bounded prefix
/// of it) is registered as a re-export, substitute the prefix with the
/// re-export's target and return the new canonical.
///
/// **THE single splice point** for re-export substitution. Both the
/// canonicalisation gate (`bindings::canonicalise_workspace_path`)
/// AND the defense-in-depth post-pass (`collect_reexport_rewrites`)
/// route through this helper, so any future site that needs to flip
/// a re-exported canonical back to its declaration follows the same
/// longest-prefix-match semantics.
///
/// Returns `Some(new_canonical)` when a rewrite applies, `None`
/// otherwise. Caller-side: if `None`, keep the input as-is.
///
/// Examples (input → output):
/// - `crate::application::Trait` (in map) → `crate::application::trait_mod::Trait`
/// - `crate::application::Trait::method` (prefix in map) →
///   `crate::application::trait_mod::Trait::method`
/// - `crate::application::Struct::new` (prefix in map) →
///   `crate::application::module::Struct::new`
/// - `std::sync::Arc` (no match) → `None`
///
/// Operation: longest-prefix-walk, no own calls.
pub(crate) fn canonicalise_reexport_path(canonical: &str, map: &ReexportMap) -> Option<String> {
    if let Some(target) = map.get(canonical) {
        return Some(target.clone());
    }
    let mut idx = canonical.len();
    while let Some(sep) = canonical[..idx].rfind("::") {
        let prefix = &canonical[..sep];
        if let Some(target) = map.get(prefix) {
            return Some(format!("{target}{}", &canonical[sep..]));
        }
        idx = sep;
    }
    None
}

/// Single substitution point for the workspace re-export map, called
/// as the final step of `canonicalise_workspace_path`. Only fires on
/// `crate::`-rooted resolutions (extern paths pass through), so every
/// gate-routed site automatically gets DECL canonicals.
pub(crate) fn apply_reexport_substitution(
    resolved: Vec<String>,
    reexports: Option<&ReexportMap>,
) -> Vec<String> {
    let Some(map) = reexports else {
        return resolved;
    };
    if resolved.first().map(String::as_str) != Some("crate") {
        return resolved;
    }
    let joined = resolved.join("::");
    match canonicalise_reexport_path(&joined, map) {
        Some(rewritten) => rewritten.split("::").map(String::from).collect(),
        None => resolved,
    }
}

// qual:api
/// Apply the re-export map to every callee canonical in the graph,
/// rewriting both `forward` and `reverse`. Edges whose callee isn't
/// in the map are left alone.
///
/// Defense-in-depth: the canonicalisation gate
/// (`bindings::canonicalise_workspace_path`) routes every user-written
/// path through `canonicalise_reexport_path` before insertion, so in
/// principle no graph edge should still carry a re-export-rooted
/// callee by the time this post-pass runs. The post-pass survives as
/// a safety net for any future emission site that bypasses the gate.
pub(super) fn rewrite_reexport_edges(graph: &mut CallGraph, reexports: &ReexportMap) {
    if reexports.is_empty() {
        return;
    }
    let rewrites = collect_reexport_rewrites(graph, reexports);
    for (caller, old, new) in rewrites {
        apply_edge_rewrite(graph, &caller, &old, new);
    }
}

/// Per-file walk: visit `Item::Use` items (visibility != Inherited)
/// and inline `Item::Mod` blocks. For each pub `use` leaf, register
/// `(reexport_canonical → target_canonical)` in `map`. Operation:
/// closure-hidden recursion for IOSP leniency.
// qual:recursive
fn walk_pub_uses(
    items: &[syn::Item],
    mod_stack: &mut Vec<String>,
    file: &FileScope<'_>,
    map: &mut ReexportMap,
) {
    let recurse = |inner: &[syn::Item], stack: &mut Vec<String>, map: &mut ReexportMap| {
        walk_pub_uses(inner, stack, file, map);
    };
    for item in items {
        if let syn::Item::Use(u) = item {
            if !is_pub_use(&u.vis) {
                continue;
            }
            let mut leaves: Vec<(Vec<String>, String)> = Vec::new();
            collect_pub_use_leaves(&[], &u.tree, &mut leaves);
            register_pub_use_leaves(&leaves, u.leading_colon.is_some(), file, mod_stack, map);
        }
        if let syn::Item::Mod(m) = item {
            if has_cfg_test(&m.attrs) {
                continue;
            }
            if let Some((_, inner)) = m.content.as_ref() {
                mod_stack.push(m.ident.to_string());
                recurse(inner, mod_stack, map);
                mod_stack.pop();
            }
        }
    }
}

/// Resolve each leaf against the file scope and insert into the map.
/// `leading_colon_set` propagates the absolute-path gate from the
/// surrounding `pub use ::ext::Foo;` statement so extern-rooted
/// re-exports don't false-route to workspace symbols.
/// Operation: per-leaf canonicalisation + reexport-canonical assembly.
fn register_pub_use_leaves(
    leaves: &[(Vec<String>, String)],
    leading_colon_set: bool,
    file: &FileScope<'_>,
    mod_stack: &[String],
    map: &mut ReexportMap,
) {
    for (path_segs, name) in leaves {
        let Some(target) = canonicalise_workspace_path(
            path_segs,
            leading_colon_set,
            &CanonScope {
                file,
                mod_stack,
                reexports: None,
            },
        ) else {
            continue;
        };
        let reexport = build_reexport_canonical(file.path, mod_stack, name);
        let target_str = target.join("::");
        if reexport == target_str {
            continue;
        }
        map.insert(reexport, target_str);
    }
}

/// `crate::<file_module>::<mod_stack>::<name>` — the canonical form
/// a caller writes when invoking the re-exported item via the
/// importing module. Operation: segment assembly.
fn build_reexport_canonical(file_path: &str, mod_stack: &[String], name: &str) -> String {
    let mut segs: Vec<String> = vec!["crate".to_string()];
    segs.extend(file_to_module_segments(file_path));
    segs.extend(mod_stack.iter().cloned());
    segs.push(name.to_string());
    segs.join("::")
}

/// True iff `vis` is anything other than `Inherited`. `pub`,
/// `pub(crate)`, `pub(super)`, `pub(in path)` all create re-exports;
/// only the bare-private form skips registration.
fn is_pub_use(vis: &syn::Visibility) -> bool {
    !matches!(vis, syn::Visibility::Inherited)
}

/// Walk a `use` tree and emit `(prefix_segments, visible_name)` per
/// leaf. Mirrors `collect_alias_entries` but returns leaf-name +
/// path-to-resolve separately so the caller can compute both
/// reexport and target canonicals.
/// Operation: recursive AST walk, own calls hidden in closure.
// qual:recursive
fn collect_pub_use_leaves(
    prefix: &[String],
    tree: &syn::UseTree,
    out: &mut Vec<(Vec<String>, String)>,
) {
    let recurse = |p: &[String], t: &syn::UseTree, o: &mut Vec<(Vec<String>, String)>| {
        collect_pub_use_leaves(p, t, o);
    };
    match tree {
        syn::UseTree::Path(p) => {
            let mut next = prefix.to_vec();
            next.push(p.ident.to_string());
            recurse(&next, &p.tree, out);
        }
        syn::UseTree::Name(n) => {
            let ident = n.ident.to_string();
            if ident == "self" {
                if let Some(last) = prefix.last().cloned() {
                    out.push((prefix.to_vec(), last));
                }
            } else {
                let mut full = prefix.to_vec();
                full.push(ident.clone());
                out.push((full, ident));
            }
        }
        syn::UseTree::Rename(r) => {
            if r.ident == "self" {
                out.push((prefix.to_vec(), r.rename.to_string()));
            } else {
                let mut full = prefix.to_vec();
                full.push(r.ident.to_string());
                out.push((full, r.rename.to_string()));
            }
        }
        syn::UseTree::Glob(_) => {
            // `pub use foo::*` re-exports everything in `foo`. Without
            // walking foo's items we can't enumerate the leaves, so
            // skip — re-export resolution stays best-effort. Affected
            // callers must use the direct path.
        }
        syn::UseTree::Group(g) => {
            for sub in &g.items {
                recurse(prefix, sub, out);
            }
        }
    }
}

/// Iteratively resolve re-export chains: if `target` is itself a
/// re-export key, replace with its target. Bounded by
/// `MAX_REEXPORT_CHAIN_DEPTH` so a malformed cyclic input can't
/// hang. Operation.
fn flatten_chains(map: &mut ReexportMap) {
    let keys: Vec<String> = map.keys().cloned().collect();
    for key in keys {
        let mut current = map.get(&key).cloned().unwrap_or_default();
        let mut depth = 0;
        while depth < MAX_REEXPORT_CHAIN_DEPTH {
            match map.get(&current) {
                Some(next) if next != &current => {
                    current = next.clone();
                    depth += 1;
                }
                _ => break,
            }
        }
        map.insert(key, current);
    }
}

/// Collect rewrites for every edge whose callee canonical (or a
/// strict `::`-bounded prefix thereof) matches a re-export key. Routes
/// through `canonicalise_reexport_path` so composite
/// `<reexport_canonical>::<method>` shapes (synthetic anchor /
/// associated-fn keys) are caught alongside plain free-fn callees.
/// Operation.
fn collect_reexport_rewrites(
    graph: &CallGraph,
    reexports: &ReexportMap,
) -> Vec<(String, String, String)> {
    let mut rewrites: Vec<(String, String, String)> = Vec::new();
    for (caller, callees) in &graph.forward {
        for callee in callees {
            if let Some(target) = canonicalise_reexport_path(callee, reexports) {
                rewrites.push((caller.clone(), callee.clone(), target));
            }
        }
    }
    rewrites
}
