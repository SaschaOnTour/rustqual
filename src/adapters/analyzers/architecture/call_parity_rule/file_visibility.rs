//! Per-file root-visibility pre-pass.
//!
//! Whether a file like `src/foo/internal.rs` participates in the
//! call-parity public surface depends on the **chain** of `mod X`
//! declarations from the crate root down. The chain may mix
//! file-backed mods (`mod X;`) and inline mods (`mod X { … }`) at
//! every level — both kinds gate visibility.
//!
//! Three semantic refinements:
//!
//! 1. **Crate-root `mod X;` is crate-visible**, even without `pub`.
//!    Call-parity is an *internal* architecture check, not an
//!    external-API surface check. Only nested non-root `mod foo`
//!    (inline or file-backed, no `pub`) marks the subtree as a
//!    private helper.
//!
//! 2. **Visibility composes recursively along the ancestor chain**,
//!    crossing inline/file-backed boundaries seamlessly. A private
//!    inline ancestor hides every descendant even if some inner
//!    `pub mod` says otherwise.
//!
//! 3. **Library and binary crate roots stay distinct.** A workspace
//!    with both `src/lib.rs` and `src/main.rs` has two independent
//!    module trees — a file is visible iff **at least one** root
//!    declares it (transitively) as visible. Files declared in no
//!    root tree (orphans / stale files) are treated as hidden, not
//!    fallback-visible: their `pub fn`s would otherwise be flagged
//!    by Check B/D for the wrong reason.

use std::collections::HashMap;

use crate::adapters::analyzers::architecture::forbidden_rule::file_to_module_segments;

/// Map every workspace file to whether its file-root contents are
/// reachable as call-parity public surface.
pub(crate) fn collect_file_root_visibility(files: &[(&str, &syn::File)]) -> HashMap<String, bool> {
    let segs_to_path: HashMap<Vec<String>, &str> = files
        .iter()
        .map(|(path, _)| (file_to_module_segments(path), *path))
        .collect();
    let ctx = WalkCtx {
        files,
        segs_to_path: &segs_to_path,
    };
    let crate_roots: Vec<&[syn::Item]> = files
        .iter()
        .filter(|(p, _)| matches!(*p, "src/lib.rs" | "src/main.rs"))
        .map(|(_, ast)| ast.items.as_slice())
        .collect();
    files
        .iter()
        .map(|(path, _)| {
            let segs = file_to_module_segments(path);
            (
                (*path).to_string(),
                visible_for_file(&segs, &crate_roots, &ctx),
            )
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
fn visible_for_file(segs: &[String], crate_roots: &[&[syn::Item]], ctx: &WalkCtx<'_>) -> bool {
    if segs.is_empty() {
        return true;
    }
    if !crate_roots.is_empty() {
        return crate_roots
            .iter()
            .any(|root_items| matches!(walk_tree(root_items, segs, &[], ctx, true), Some(true)));
    }
    fallback_visibility(segs, ctx)
}

/// Trivial: closure-hidden own calls. Used only when no `src/lib.rs`
/// / `src/main.rs` is in the workspace (typical for unit-test fixtures).
fn fallback_visibility(segs: &[String], ctx: &WalkCtx<'_>) -> bool {
    let walk = || {
        let (start_segs, start_items) = highest_file_backed_ancestor(segs, ctx)?;
        let remaining: Vec<String> = segs[start_segs.len()..].to_vec();
        let is_root = start_segs.is_empty();
        Some(matches!(
            walk_tree(start_items, &remaining, &start_segs, ctx, is_root),
            Some(true)
        ))
    };
    walk().unwrap_or(true)
}

/// Find the highest file-backed ancestor (shortest prefix) of `segs`
/// that exists in the workspace.
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
/// chain at `items`. Three-state result:
/// - `Some(true)`  — this tree resolves the segment as visible
/// - `Some(false)` — this tree finds a private link in the chain
/// - `None`        — this tree does not declare the segment at all
///   (the file is not part of this module tree)
fn walk_tree(
    items: &[syn::Item],
    remaining: &[String],
    seen_so_far: &[String],
    ctx: &WalkCtx<'_>,
    is_crate_root_level: bool,
) -> Option<bool> {
    let step = || -> Option<Option<bool>> {
        let first = remaining.first()?;
        let m = find_mod_decl(items, first);
        let Some(m) = m else {
            return Some(None);
        };
        if !mod_decl_visible(m, is_crate_root_level) {
            return Some(Some(false));
        }
        let rest = &remaining[1..];
        if rest.is_empty() {
            return Some(Some(true));
        }
        Some(descend_into_mod(m, rest, seen_so_far, first, ctx))
    };
    step().flatten()
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

/// Trivial: closure-hidden own calls.
fn descend_into_mod(
    m: &syn::ItemMod,
    rest: &[String],
    seen_so_far: &[String],
    first: &str,
    ctx: &WalkCtx<'_>,
) -> Option<bool> {
    let descend = || {
        let mut next_seen = seen_so_far.to_vec();
        next_seen.push(first.to_string());
        if let Some((_, inner)) = m.content.as_ref() {
            return Some(walk_tree(inner, rest, &next_seen, ctx, false));
        }
        let child_items = ctx.items_for(&next_seen)?;
        Some(walk_tree(child_items, rest, &next_seen, ctx, false))
    };
    descend().flatten()
}
