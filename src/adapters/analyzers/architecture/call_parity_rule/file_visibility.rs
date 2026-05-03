//! Per-file root-visibility pre-pass.
//!
//! A file like `src/foo/internal.rs` is reachable as public surface
//! iff its parent file (`src/foo.rs` or `src/foo/mod.rs`) declares
//! `pub mod internal;`. A bare `mod internal;` (without `pub`) keeps
//! the whole file private to the parent module — without this gate,
//! `pub fn helper()` inside `internal.rs` would be recorded as a
//! target-layer pub fn and Check B/D would require adapter coverage
//! for a private helper.

use std::collections::HashMap;

use crate::adapters::analyzers::architecture::forbidden_rule::file_to_module_segments;

/// Map every workspace file to whether its file-root contents are
/// reachable as public surface from the crate root.
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
            let visible = file_root_visibility(&segs, files, &segs_to_path);
            ((*path).to_string(), visible)
        })
        .collect()
}

fn file_root_visibility(
    segs: &[String],
    files: &[(&str, &syn::File)],
    segs_to_path: &HashMap<Vec<String>, &str>,
) -> bool {
    if segs.is_empty() {
        return true; // crate root file
    }
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
    parent_mod_decl_is_visible(&parent_ast.items, leaf).unwrap_or(true)
}

fn parent_mod_decl_is_visible(items: &[syn::Item], target: &str) -> Option<bool> {
    items.iter().find_map(|item| match item {
        syn::Item::Mod(m) if m.ident == target && m.content.is_none() => {
            Some(super::pub_fns_visibility::is_visible(&m.vis))
        }
        _ => None,
    })
}
