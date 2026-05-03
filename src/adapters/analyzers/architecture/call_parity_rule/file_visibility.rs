//! Per-file root-visibility pre-pass.
//!
//! Whether a file like `src/foo/internal.rs` participates in the
//! call-parity public surface depends on the **chain** of `mod X;`
//! declarations from the crate root down. Two semantic refinements:
//!
//! 1. **Crate-root `mod X;` is crate-visible**, even without `pub`.
//!    `src/lib.rs` typically writes `mod cli; mod application;` —
//!    sibling modules still reach them via `crate::cli::…`, and
//!    call-parity is an *internal* architecture check, not an
//!    external-API surface check. Only nested non-root `mod foo;`
//!    (no `pub`) marks the subtree as a private helper.
//!
//! 2. **Visibility composes recursively along the ancestor chain.**
//!    `mod internal;` (private) at depth 1 + `pub mod deep;` at depth
//!    2 → `deep` is hidden because the `internal` ancestor is private,
//!    even though its direct parent says `pub`. The pre-pass walks
//!    every ancestor and short-circuits to `false` on the first
//!    non-visible link.

use std::collections::HashMap;

use crate::adapters::analyzers::architecture::forbidden_rule::file_to_module_segments;

/// Map every workspace file to whether its file-root contents are
/// reachable as call-parity public surface.
///
/// Crate-root files (`src/lib.rs`, `src/main.rs`) are always visible.
/// Files with no parent in the workspace (orphaned) default to
/// visible — the call-parity layer config decides whether they
/// participate at all.
pub(crate) fn collect_file_root_visibility(files: &[(&str, &syn::File)]) -> HashMap<String, bool> {
    let segs_to_path: HashMap<Vec<String>, &str> = files
        .iter()
        .map(|(path, _)| (file_to_module_segments(path), *path))
        .collect();
    files
        .iter()
        .map(|(path, _)| {
            let segs = file_to_module_segments(path);
            let visible = ancestor_chain_visible(&segs, files, &segs_to_path);
            ((*path).to_string(), visible)
        })
        .collect()
}

/// True iff every `mod` link from the crate root down to `segs`
/// resolves to a visible declaration. Short-circuits on the first
/// private ancestor link.
fn ancestor_chain_visible(
    segs: &[String],
    files: &[(&str, &syn::File)],
    segs_to_path: &HashMap<Vec<String>, &str>,
) -> bool {
    if segs.is_empty() {
        return true; // crate root
    }
    let mut current = segs.to_vec();
    while !current.is_empty() {
        if !direct_link_visible(&current, files, segs_to_path) {
            return false;
        }
        current.pop();
    }
    true
}

/// True iff the parent of `segs` declares `mod <leaf>` with adequate
/// visibility for an internal call-parity check. Crate-root parents
/// (`src/lib.rs` / `src/main.rs`) treat any `mod X;` as visible —
/// `Inherited` visibility there still lets sibling modules reach the
/// declared module via `crate::X::…`. Nested parents demand an
/// explicit visibility modifier.
fn direct_link_visible(
    segs: &[String],
    files: &[(&str, &syn::File)],
    segs_to_path: &HashMap<Vec<String>, &str>,
) -> bool {
    let Some(leaf) = segs.last() else {
        return true;
    };
    let parent_segs: Vec<String> = segs[..segs.len() - 1].to_vec();
    let Some(parent_path) = segs_to_path.get(&parent_segs) else {
        return true; // parent not in workspace — default visible
    };
    let Some((_, parent_ast)) = files.iter().find(|(p, _)| p == parent_path) else {
        return true;
    };
    let parent_is_crate_root = parent_segs.is_empty();
    parent_mod_decl_visible(&parent_ast.items, leaf, parent_is_crate_root).unwrap_or(true)
}

fn parent_mod_decl_visible(
    items: &[syn::Item],
    target: &str,
    parent_is_crate_root: bool,
) -> Option<bool> {
    items.iter().find_map(|item| match item {
        syn::Item::Mod(m) if m.ident == target && m.content.is_none() => {
            if parent_is_crate_root {
                // Crate-root `mod X;` is crate-visible regardless of
                // its `pub` modifier — sibling modules still reach X.
                Some(true)
            } else {
                Some(super::pub_fns_visibility::is_visible(&m.vis))
            }
        }
        _ => None,
    })
}
