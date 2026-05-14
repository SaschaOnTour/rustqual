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

use super::bindings::{canonicalise_type_segments_in_scope, CanonScope};
use super::local_symbols::FileScope;
use super::workspace_graph::{apply_edge_rewrite, CallGraph};
use crate::adapters::analyzers::architecture::forbidden_rule::file_to_module_segments;
use crate::adapters::shared::cfg_test::has_cfg_test;
use std::collections::HashMap;

/// Hard cap on chained-reexport flattening to prevent pathological
/// cycles (`pub use a::b; pub use b::a` — illegal in Rust but the
/// detector mustn't loop). Real-world chains bottom out under 4.
const MAX_REEXPORT_CHAIN_DEPTH: usize = 16;

// qual:api
/// Walk every `pub use` (and `pub(crate)`/`pub(super)`/`pub(in path)`
/// use) leaf across the workspace and produce a map from
/// reexport-site canonical → original-definition canonical. Chained
/// re-exports are flattened iteratively.
pub(super) fn collect_reexport_map(
    files: &[(&str, &syn::File)],
    workspace_files: &HashMap<String, FileScope<'_>>,
) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();
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
/// Apply the re-export map to every callee canonical in the graph,
/// rewriting both `forward` and `reverse`. Edges whose callee isn't
/// in the map are left alone.
pub(super) fn rewrite_reexport_edges(graph: &mut CallGraph, reexports: &HashMap<String, String>) {
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
    map: &mut HashMap<String, String>,
) {
    let recurse =
        |inner: &[syn::Item], stack: &mut Vec<String>, map: &mut HashMap<String, String>| {
            walk_pub_uses(inner, stack, file, map);
        };
    for item in items {
        if let syn::Item::Use(u) = item {
            if !is_pub_use(&u.vis) {
                continue;
            }
            let mut leaves: Vec<(Vec<String>, String)> = Vec::new();
            collect_pub_use_leaves(&[], &u.tree, &mut leaves);
            register_pub_use_leaves(&leaves, file, mod_stack, map);
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
/// Operation: per-leaf canonicalisation + reexport-canonical assembly.
fn register_pub_use_leaves(
    leaves: &[(Vec<String>, String)],
    file: &FileScope<'_>,
    mod_stack: &[String],
    map: &mut HashMap<String, String>,
) {
    for (path_segs, name) in leaves {
        let Some(target) =
            canonicalise_type_segments_in_scope(path_segs, &CanonScope { file, mod_stack })
        else {
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
fn flatten_chains(map: &mut HashMap<String, String>) {
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

/// Collect rewrites for every edge whose callee is in `reexports`.
/// Operation.
fn collect_reexport_rewrites(
    graph: &CallGraph,
    reexports: &HashMap<String, String>,
) -> Vec<(String, String, String)> {
    let mut rewrites: Vec<(String, String, String)> = Vec::new();
    for (caller, callees) in &graph.forward {
        for callee in callees {
            if let Some(target) = reexports.get(callee) {
                rewrites.push((caller.clone(), callee.clone(), target.clone()));
            }
        }
    }
    rewrites
}
