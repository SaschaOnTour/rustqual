//! The `dead_code` lint level a file inherits from the module that declares it.
//!
//! Rust resolves lint levels innermost-first and does not stop at a file
//! boundary: `#![allow(dead_code)]` at the top of `inner/mod.rs`, or
//! `#[allow(dead_code)] mod inner;` in the parent, covers everything the module
//! contains — including the declarations that live in its child *files*.
//!
//! `allow_scope` tracks the level within one file, which is where the
//! attributes it reads are. What it could not see is the level a file arrives
//! with, so a declaration the author had excused one level up was reported —
//! a false finding by DRY-002's and DRY-006's own documented rule, and the
//! expensive direction.
//!
//! The walk is the shared one (`shared::cfg_test_files::external_mods` +
//! `child_paths`), the same that propagates `#[cfg(test)]` down the module
//! tree, so the two cannot disagree about which file a `mod` names.

use std::collections::HashMap;

use super::allow_scope::{level_under, DeadCodeLevel};
use crate::adapters::shared::cfg_test_files::{external_mods, ChildPathResolver};

/// The level each file inherits, keyed by path. Absent means `Report` — a file
/// no module chain reaches inherits nothing.
pub(crate) type InheritedLevels = HashMap<String, DeadCodeLevel>;

/// Resolve the inherited level for every file in the parsed set.
/// Integration: resolver + fixpoint over the module tree.
pub(crate) fn inherited_levels(parsed: &[(String, String, syn::File)]) -> InheritedLevels {
    let refs: Vec<(&str, &syn::File)> = parsed.iter().map(|(p, _, f)| (p.as_str(), f)).collect();
    let resolver = ChildPathResolver::from_parsed(&refs);
    propagate(&refs, &resolver)
}

/// Push each file's own effective level onto the files its `mod` declarations
/// name, until nothing changes.
///
/// Bounded by the number of files: a level only ever moves to a strictly more
/// permissive value, and there are three. A cyclic `#[path]` arrangement
/// therefore settles rather than looping.
/// Operation: fixpoint over the declarations, own calls hidden in the closures.
fn propagate(refs: &[(&str, &syn::File)], resolver: &ChildPathResolver<'_>) -> InheritedLevels {
    let mut levels = InheritedLevels::new();
    for _ in 0..=refs.len() {
        let edges: Vec<(String, DeadCodeLevel)> = refs
            .iter()
            .flat_map(|(path, file)| {
                let inherited = levels.get(*path).copied().unwrap_or(DeadCodeLevel::Report);
                let own = level_under(inherited, &file.attrs);
                external_mods(&file.items, &[], false)
                    .into_iter()
                    .filter_map(|e| {
                        resolver
                            .resolve(path, &e.inline_stack, e.item)
                            .map(|child| (child, level_under(own, &e.item.attrs)))
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let fresh: Vec<(String, DeadCodeLevel)> = edges
            .into_iter()
            .filter(|(child, level)| levels.get(child) != Some(level))
            .collect();
        if fresh.is_empty() {
            break;
        }
        levels.extend(fresh);
    }
    levels
}
