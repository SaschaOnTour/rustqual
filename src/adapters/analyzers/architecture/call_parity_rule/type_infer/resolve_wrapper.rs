//! Wrapper-name decision for the type resolver.
//!
//! Splits cleanly from `resolve.rs` because the decision logic
//! (single-segment shortcut, explicit-stdlib shortcut, scoped
//! canonicalisation with stdlib-prefix verification) doesn't share
//! call edges with the rest of the resolver — it's a self-contained
//! "is this path a wrapper" probe.

use super::super::bindings::{canonicalise_type_segments_in_scope, CanonScope};
use super::resolve::{is_stdlib_prefixed, is_user_transparent, ResolveContext, WRAPPER_NAMES};

/// Decide the wrapper-arm name for `path`. Returns `Some(name)` when
/// the path should be dispatched as a wrapper, `None` otherwise.
///
/// Resolution flow (canonicalise-first, fallbacks for unresolvable
/// paths):
///   1. Full canonicalisation. When the path resolves through the
///      alias / local-symbol / crate-root pipeline, the canonical
///      authoritatively decides: stdlib-prefixed + wrapper leaf, or
///      user-transparent leaf → wrapper. Anything else → not a
///      wrapper. This catches the shadow case
///      (`use crate::wrap::Arc;` then `Arc<T>`) — the canonical
///      points to the local type, not stdlib.
///   2. User-transparent leaf-name match. The user opts into "any
///      path ending in `State` is transparent", so external-crate
///      forms like `axum::extract::State<T>` (which the
///      canonicaliser can't reach) still peel.
///   3. Bare wrapper convention. `Result<T>`, `Option<T>`,
///      `Arc<T>` without an active `use` (or with the standard
///      stdlib `use`) work as expected — the canonicaliser fails
///      cleanly in those cases.
///   4. Explicit stdlib qualification (`std::sync::Arc<T>`,
///      `core::option::Option<T>`) for callers that fully qualify
///      without aliasing.
///
/// Operation.
pub(super) fn identify_wrapper_name(
    path: &syn::Path,
    raw_name: &str,
    ctx: &ResolveContext<'_>,
) -> Option<String> {
    let scope = CanonScope {
        file: ctx.file,
        mod_stack: ctx.mod_stack,
    };
    let segs: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    if let Some(canonical) = canonicalise_type_segments_in_scope(&segs, &scope) {
        let last_seg = canonical.last()?;
        let stdlib_match =
            is_stdlib_prefixed(&canonical) && WRAPPER_NAMES.contains(&last_seg.as_str());
        return if stdlib_match || is_user_transparent(last_seg, ctx) {
            Some(last_seg.clone())
        } else {
            None
        };
    }
    if is_user_transparent(raw_name, ctx) {
        return Some(raw_name.to_string());
    }
    let single = path.segments.len() == 1;
    if single && WRAPPER_NAMES.contains(&raw_name) {
        return Some(raw_name.to_string());
    }
    let first_seg = path.segments.first().map(|s| s.ident.to_string());
    let explicit_stdlib = matches!(first_seg.as_deref(), Some("std" | "core" | "alloc"));
    if explicit_stdlib && WRAPPER_NAMES.contains(&raw_name) {
        return Some(raw_name.to_string());
    }
    None
}
