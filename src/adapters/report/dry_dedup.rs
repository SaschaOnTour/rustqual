//! Shared dedup helper used by HTML and text DRY renderers.
//!
//! `project_dry` emits one `DryFinding` per group participant; renderers
//! that show "one row per group" need to dedupe by a per-variant key.
//! This single helper is the canonical place for that logic so HTML and
//! text don't drift apart.

use std::collections::HashSet;
use std::hash::Hash;

use crate::domain::findings::DryFinding;

/// Walk `findings`, skip suppressed entries, run `extract` on each, and
/// keep only the first occurrence per key. Generic in the key type so
/// each variant can choose what defines a unique group (e.g. duplicates
/// key on participant locations alone, repeated-match keys on
/// `(enum_name, sorted locations)` to match JSON's grouping).
pub(super) fn dedup_by_key<'a, T: 'a, K: Hash + Eq + 'a>(
    findings: &'a [DryFinding],
    extract: impl Fn(&'a DryFinding) -> Option<(T, K)>,
) -> Vec<T> {
    let mut seen: HashSet<K> = HashSet::new();
    let mut groups = Vec::new();
    findings
        .iter()
        .filter(|f| !f.common.suppressed)
        .for_each(|f| {
            if let Some((row, key)) = extract(f) {
                if seen.insert(key) {
                    groups.push(row);
                }
            }
        });
    groups
}
