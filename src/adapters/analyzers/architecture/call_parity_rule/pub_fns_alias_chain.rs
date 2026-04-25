//! Workspace-wide alias-chain pre-pass.
//!
//! Walks every file (including private modules) and records each
//! `type Alias = Target;` declaration as a `(alias_canonical →
//! target_canonical)` edge. The visibility collector chases this map
//! after registering an alias's immediate target so chains like
//! `pub type Public = Inner; type Inner = private::Hidden;` reach
//! the source type even when intermediate aliases are private.

use super::bindings::{canonicalise_type_segments_in_scope, CanonScope};
use super::local_symbols::{collect_local_symbols_scoped, FileScope, LocalSymbols};
use super::pub_fns_visibility::{canonical_for_decl, peel_to_inner_path};
use crate::adapters::shared::cfg_test::has_cfg_test;
use crate::adapters::shared::use_tree::gather_alias_map_scoped;
use std::collections::{HashMap, HashSet};

/// Build the workspace-wide alias-chain map. Per-file delegate to the
/// unconditional walker. Operation.
pub(super) fn collect_alias_chain(
    files: &[(&str, &syn::File)],
    cfg_test_files: &HashSet<String>,
    aliases_per_file: &HashMap<String, HashMap<String, Vec<String>>>,
    crate_root_modules: &HashSet<String>,
    transparent_wrappers: &HashSet<String>,
) -> HashMap<String, String> {
    let mut chain = HashMap::new();
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
        walk_alias_chain(
            &ast.items,
            &[],
            &file_scope,
            transparent_wrappers,
            &mut chain,
        );
    }
    chain
}

/// Recursive walk that records every `type X = Y;` declaration —
/// regardless of `X`'s visibility or its enclosing module's
/// visibility — into the alias-chain map. Operation: closure-hidden
/// descent into all `mod` blocks (cfg-test still skipped).
// qual:recursive
fn walk_alias_chain(
    items: &[syn::Item],
    mod_stack: &[String],
    file_scope: &FileScope<'_>,
    transparent_wrappers: &HashSet<String>,
    chain: &mut HashMap<String, String>,
) {
    let recurse = |inner: &[syn::Item], next: &[String], chain: &mut HashMap<String, String>| {
        walk_alias_chain(inner, next, file_scope, transparent_wrappers, chain);
    };
    for item in items {
        match item {
            syn::Item::Type(t) => {
                let alias_canonical =
                    canonical_for_decl(file_scope.path, mod_stack, &t.ident.to_string());
                if let Some(target) = resolve_alias_target_canonical(
                    &t.ty,
                    file_scope,
                    mod_stack,
                    transparent_wrappers,
                ) {
                    chain.insert(alias_canonical, target);
                }
            }
            syn::Item::Mod(m) if !has_cfg_test(&m.attrs) => {
                if let Some((_, inner)) = m.content.as_ref() {
                    let mut next = mod_stack.to_vec();
                    next.push(m.ident.to_string());
                    recurse(inner, &next, chain);
                }
            }
            _ => {}
        }
    }
}

/// Peel-and-canonicalise an alias's target type, returning the
/// resolved canonical path joined as a string. Shared by the
/// chain-builder and the visibility walker so both agree on what
/// `pub type Public = Box<private::Hidden>` reduces to. Operation.
pub(super) fn resolve_alias_target_canonical(
    ty: &syn::Type,
    file_scope: &FileScope<'_>,
    mod_stack: &[String],
    transparent_wrappers: &HashSet<String>,
) -> Option<String> {
    let p = peel_to_inner_path(ty, transparent_wrappers)?;
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
    canonicalise_type_segments_in_scope(&segs, &scope).map(|c| c.join("::"))
}

/// Follow an alias chain from `start` through `alias_chain` until a
/// fixed point or cycle is reached, inserting every intermediate
/// canonical into `out`. `seen` guards against `type A = B; type B
/// = A;` cycles. Operation.
pub(super) fn chase_alias_chain(
    start: &str,
    alias_chain: &HashMap<String, String>,
    out: &mut HashSet<String>,
) {
    let mut current = start.to_string();
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(current.clone());
    while let Some(next) = alias_chain.get(&current) {
        if !seen.insert(next.clone()) {
            break;
        }
        out.insert(next.clone());
        current = next.clone();
    }
}
