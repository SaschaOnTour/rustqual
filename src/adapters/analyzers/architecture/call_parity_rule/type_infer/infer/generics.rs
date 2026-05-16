//! Turbofish as return-type override for generic fn / method calls.
//!
//! Two call shapes flow through the same helper:
//! - Path-call (`get::<Session>()`): turbofish lives on the last
//!   path segment's `PathArguments::AngleBracketed`. Restricted to
//!   single-segment paths so `Vec::<u32>::new()` (turbofish on the
//!   type segment, not the method) doesn't over-approximate.
//! - Method-call (`s.method::<Session>()`): turbofish lives directly
//!   on `ExprMethodCall.turbofish`.
//!
//! Both shapes use `resolve_with_turbofish_override`: if the index-
//! inferred return type is a `TraitBound` (generic-param return), the
//! turbofish substitution is strictly more specific and wins; if the
//! index says nothing, the turbofish is the fallback; otherwise the
//! index wins.

use super::super::canonical::CanonicalType;
use super::super::resolve::{resolve_type, ResolveContext};
use super::super::self_subst::substitute_bare_self;
use super::InferContext;

// qual:api
/// Combine an index-inferred return type with an optional explicit
/// turbofish substitution. The single source of truth for the
/// "TraitBound + explicit turbofish → turbofish wins" rule used by
/// both `infer_call` (path-style) and `infer_method_call`
/// (method-style). Integration.
pub(super) fn resolve_with_turbofish_override(
    inferred: Option<CanonicalType>,
    turbofish: Option<&syn::AngleBracketedGenericArguments>,
    ctx: &InferContext<'_>,
) -> Option<CanonicalType> {
    let turbofish_type = turbofish.and_then(|args| resolve_first_type_arg(args, ctx));
    match (inferred, turbofish_type) {
        // Generic-param return → turbofish substitutes the concrete
        // type, which carries strictly more dispatch info than the bound.
        (Some(CanonicalType::TraitBound(_)), Some(tf)) => Some(tf),
        // Concrete index return wins; turbofish is the fallback when
        // the index has nothing.
        (Some(t), _) => Some(t),
        (None, tf) => tf,
    }
}

// qual:api
/// Extract the turbofish arguments from a path's last segment if
/// the path is a single-segment generic call (`get::<Session>()`).
/// Returns `None` for multi-segment paths or segments without
/// angle-bracketed args. Operation.
pub(super) fn path_turbofish_args(
    path: &syn::Path,
) -> Option<&syn::AngleBracketedGenericArguments> {
    if path.segments.len() != 1 {
        return None;
    }
    let syn::PathArguments::AngleBracketed(ab) = &path.segments[0].arguments else {
        return None;
    };
    Some(ab)
}

/// Resolve the first type argument inside an angle-bracketed args list
/// against the inference context, returning `None` if the type
/// collapses to `Opaque`. Operation.
fn resolve_first_type_arg(
    args: &syn::AngleBracketedGenericArguments,
    ctx: &InferContext<'_>,
) -> Option<CanonicalType> {
    let first_ty = args.args.iter().find_map(|arg| match arg {
        syn::GenericArgument::Type(t) => Some(t),
        _ => None,
    })?;
    let rctx = ResolveContext {
        file: ctx.file,
        mod_stack: ctx.mod_stack,
        type_aliases: Some(&ctx.workspace.type_aliases),
        transparent_wrappers: Some(&ctx.workspace.transparent_wrappers),
        workspace_files: ctx.workspace_files,
        alias_param_subs: None,
        generic_params: ctx.generic_params,
    };
    let resolved = match ctx.self_type.as_deref() {
        Some(impl_segs) => resolve_type(&substitute_bare_self(first_ty, impl_segs), &rctx),
        None => resolve_type(first_ty, &rctx),
    };
    if matches!(resolved, CanonicalType::Opaque) {
        return None;
    }
    Some(resolved)
}
