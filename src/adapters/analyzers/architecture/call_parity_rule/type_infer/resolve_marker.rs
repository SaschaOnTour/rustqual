//! Marker-trait detection for `dyn T1 + T2` / `impl T1 + T2` bound
//! lists.
//!
//! Splits cleanly from `resolve.rs` because the decision logic is
//! self-contained: canonicalise the path first, then skip only when
//! the canonical bound is a real stdlib-rooted marker. This mirrors
//! how `resolve_wrapper::identify_wrapper_name` decides "is this a
//! wrapper" — both routes have to be alias-aware so workspace traits
//! whose leaf happens to match a marker name (`use crate::ports::Send;`)
//! still reach dispatch.

use super::super::bindings::{canonicalise_type_segments_in_scope, CanonScope};
use super::resolve::{is_stdlib_prefixed, ResolveContext};

/// Marker traits (plus common auto-derive names) that are skipped when
/// picking the dispatch-relevant trait from a `dyn T1 + T2` bound set.
/// Kept as a const so the list is greppable and easy to extend.
const MARKER_TRAITS: &[&str] = &[
    "Send", "Sync", "Unpin", "Copy", "Clone", "Sized", "Debug", "Display",
];

/// True when `path` denotes a real stdlib marker trait and should be
/// skipped while picking the dispatch-relevant bound. Canonicalises
/// the path first so a workspace trait or alias whose leaf happens to
/// match a marker name (`use crate::ports::Send;`,
/// `dyn crate::ports::Send`) is NOT treated as a std marker. Operation.
pub(super) fn is_marker_trait(path: &syn::Path, ctx: &ResolveContext<'_>) -> bool {
    let segs: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    let Some(leaf) = segs.last() else {
        return false;
    };
    let scope = CanonScope {
        file: ctx.file,
        mod_stack: ctx.mod_stack,
    };
    if let Some(canonical) = canonicalise_type_segments_in_scope(&segs, &scope) {
        let canon_leaf = canonical.last().map(String::as_str).unwrap_or("");
        return is_stdlib_prefixed(&canonical) && MARKER_TRAITS.contains(&canon_leaf);
    }
    // Unresolvable path. Conservative fallback: bare `Send`/`Sync`/…
    // (single segment, hits std prelude) and explicit `std::marker::Send`
    // forms still count as markers; multi-segment workspace paths that
    // failed to canonicalise are treated as real bounds, not markers.
    let single_segment = segs.len() == 1;
    let explicit_stdlib = matches!(
        segs.first().map(String::as_str),
        Some("std" | "core" | "alloc")
    );
    (single_segment || explicit_stdlib) && MARKER_TRAITS.contains(&leaf.as_str())
}
