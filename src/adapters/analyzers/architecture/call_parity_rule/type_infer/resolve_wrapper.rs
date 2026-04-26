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
/// Three resolution paths, each guarded:
///   - Single-segment bare wrapper (`Arc<T>` after `use … Arc`):
///     fast path, no canonicalisation needed.
///   - Explicit stdlib qualification (`std::sync::Arc<T>`,
///     `core::option::Option<T>`): leaf is the wrapper name and the
///     prefix proves stdlib origin.
///   - Aliased / canonicalised paths (`Shared<T>`, `wrap::Shared<T>`):
///     run the scope-aware canonicaliser; auto-peel only if the
///     canonical is stdlib-prefixed and ends in a wrapper name, or
///     the leaf is in the user-transparent set.
///
/// Operation.
pub(super) fn identify_wrapper_name(
    path: &syn::Path,
    raw_name: &str,
    ctx: &ResolveContext<'_>,
) -> Option<String> {
    let single = path.segments.len() == 1;
    if single && (WRAPPER_NAMES.contains(&raw_name) || is_user_transparent(raw_name, ctx)) {
        return Some(raw_name.to_string());
    }
    let first_seg = path.segments.first().map(|s| s.ident.to_string());
    let explicit_stdlib = matches!(first_seg.as_deref(), Some("std" | "core" | "alloc"));
    if explicit_stdlib && WRAPPER_NAMES.contains(&raw_name) {
        return Some(raw_name.to_string());
    }
    let scope = CanonScope {
        file: ctx.file,
        mod_stack: ctx.mod_stack,
    };
    let segs: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    let canonical = canonicalise_type_segments_in_scope(&segs, &scope)?;
    let last_seg = canonical.last()?;
    let stdlib_match = is_stdlib_prefixed(&canonical) && WRAPPER_NAMES.contains(&last_seg.as_str());
    if stdlib_match || is_user_transparent(last_seg, ctx) {
        Some(last_seg.clone())
    } else {
        None
    }
}
