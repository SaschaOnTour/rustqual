//! Shared dedup helper used by HTML and text DRY renderers.
//!
//! `project_dry` emits one `DryFinding` per group participant; renderers
//! that show "one row per group" need to dedupe by the participant
//! location set. This single helper is the canonical place for that
//! logic so HTML and text don't drift apart.

use std::collections::HashSet;

use crate::domain::findings::DryFinding;

/// Walk `findings`, skip suppressed entries, run `extract` on each, and
/// keep only the first occurrence per location-set key.
pub(super) fn dedup_by_locations<'a, T: 'a>(
    findings: &'a [DryFinding],
    extract: impl Fn(&'a DryFinding) -> Option<(T, Vec<(String, usize)>)>,
) -> Vec<T> {
    let mut seen: HashSet<Vec<(String, usize)>> = HashSet::new();
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
