//! Per-file root-visibility pre-pass.
//!
//! Whether a file like `src/foo/internal.rs` participates in the
//! call-parity public surface depends on the **chain** of `mod X`
//! declarations from the crate root down. The chain may mix
//! file-backed mods (`mod X;`) and inline mods (`mod X { … }`) at
//! every level — both kinds gate visibility.
//!
//! Two semantic refinements:
//!
//! 1. **Crate-root `mod X;` is crate-visible**, even without `pub`.
//!    Call-parity is an *internal* architecture check, not an
//!    external-API surface check. Only nested non-root `mod foo`
//!    (inline or file-backed, no `pub`) marks the subtree as a
//!    private helper.
//!
//! 2. **Visibility composes recursively along the ancestor chain**,
//!    crossing inline/file-backed boundaries seamlessly. `mod
//!    internal { pub mod deep; }` (inline + private at depth 1)
//!    plus `src/foo/internal/deep.rs` → `deep.rs` is hidden
//!    because its inline `internal` ancestor is private, even
//!    though the inline `pub mod deep;` says public.

use std::collections::HashMap;

use crate::adapters::analyzers::architecture::forbidden_rule::file_to_module_segments;

/// Map every workspace file to whether its file-root contents are
/// reachable as call-parity public surface. Files with no ancestor
/// in the workspace default to visible.
pub(crate) fn collect_file_root_visibility(files: &[(&str, &syn::File)]) -> HashMap<String, bool> {
    let segs_to_path: HashMap<Vec<String>, &str> = files
        .iter()
        .map(|(path, _)| (file_to_module_segments(path), *path))
        .collect();
    let ctx = WalkCtx {
        files,
        segs_to_path: &segs_to_path,
    };
    files
        .iter()
        .map(|(path, _)| {
            let segs = file_to_module_segments(path);
            ((*path).to_string(), visible_for_file(&segs, &ctx))
        })
        .collect()
}

struct WalkCtx<'a> {
    files: &'a [(&'a str, &'a syn::File)],
    segs_to_path: &'a HashMap<Vec<String>, &'a str>,
}

impl<'a> WalkCtx<'a> {
    fn items_for(&self, segs: &[String]) -> Option<&'a [syn::Item]> {
        let path = self.segs_to_path.get(segs)?;
        let (_, ast) = self.files.iter().find(|(p, _)| p == path)?;
        Some(ast.items.as_slice())
    }
}

/// Trivial: closure-hidden own calls.
fn visible_for_file(segs: &[String], ctx: &WalkCtx<'_>) -> bool {
    let walk = || {
        let (start_segs, start_items) = highest_file_backed_ancestor(segs, ctx)?;
        let remaining: Vec<String> = segs[start_segs.len()..].to_vec();
        let is_root = start_segs.is_empty();
        Some(walk_segments(
            start_items,
            &remaining,
            &start_segs,
            ctx,
            is_root,
        ))
    };
    if segs.is_empty() {
        return true;
    }
    walk().unwrap_or(true)
}

/// Find the *highest* file-backed ancestor (shortest segments prefix)
/// of `segs` that exists in the workspace. Walking from the highest
/// ancestor — not the nearest — is essential: the nearest ancestor
/// would skip private-mod links at higher levels and let a hidden
/// subtree leak into the public surface. Operation: prefix iteration
/// with closure-hidden lookup.
fn highest_file_backed_ancestor<'a>(
    segs: &[String],
    ctx: &WalkCtx<'a>,
) -> Option<(Vec<String>, &'a [syn::Item])> {
    let try_lookup = |candidate: &[String]| -> Option<(Vec<String>, &'a [syn::Item])> {
        ctx.items_for(candidate)
            .map(|items| (candidate.to_vec(), items))
    };
    if let Some(found) = try_lookup(&[]) {
        return Some(found);
    }
    let mut candidate: Vec<String> = Vec::new();
    for seg in &segs[..segs.len() - 1] {
        candidate.push(seg.clone());
        if let Some(found) = try_lookup(&candidate) {
            return Some(found);
        }
    }
    None
}

/// Trivial: closure-hidden own calls. Walk one level of the `mod`
/// chain at `items`, descending into `remaining[0]`.
fn walk_segments(
    items: &[syn::Item],
    remaining: &[String],
    seen_so_far: &[String],
    ctx: &WalkCtx<'_>,
    is_crate_root_level: bool,
) -> bool {
    let step = || -> Option<bool> {
        let first = remaining.first()?;
        let m = find_mod_decl(items, first)?;
        if !mod_decl_visible(m, is_crate_root_level) {
            return Some(false);
        }
        let rest = &remaining[1..];
        if rest.is_empty() {
            return Some(true);
        }
        Some(descend_into_mod(m, rest, seen_so_far, first, ctx))
    };
    step().unwrap_or(true)
}

fn find_mod_decl<'a>(items: &'a [syn::Item], target: &str) -> Option<&'a syn::ItemMod> {
    items.iter().find_map(|item| match item {
        syn::Item::Mod(m) if m.ident == target => Some(m),
        _ => None,
    })
}

fn mod_decl_visible(m: &syn::ItemMod, is_crate_root_level: bool) -> bool {
    if is_crate_root_level {
        true
    } else {
        super::pub_fns_visibility::is_visible(&m.vis)
    }
}

/// Trivial: closure-hidden own calls. Descend into either an inline
/// `mod m { … }`'s items or the corresponding file-backed `mod m;`
/// child file's items, then continue the walk.
fn descend_into_mod(
    m: &syn::ItemMod,
    rest: &[String],
    seen_so_far: &[String],
    first: &str,
    ctx: &WalkCtx<'_>,
) -> bool {
    let descend = || -> Option<bool> {
        let mut next_seen = seen_so_far.to_vec();
        next_seen.push(first.to_string());
        if let Some((_, inner)) = m.content.as_ref() {
            return Some(walk_segments(inner, rest, &next_seen, ctx, false));
        }
        let child_items = ctx.items_for(&next_seen)?;
        Some(walk_segments(child_items, rest, &next_seen, ctx, false))
    };
    descend().unwrap_or(true)
}
