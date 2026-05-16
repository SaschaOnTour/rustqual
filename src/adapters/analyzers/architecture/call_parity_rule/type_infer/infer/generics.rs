//! Turbofish as return-type override for generic fn / method calls.
//!
//! Two call shapes flow through this module:
//! - Path-call (`get::<Session>()`): turbofish lives on the last
//!   path segment's `PathArguments::AngleBracketed`. Restricted to
//!   single-segment paths so `Vec::<u32>::new()` (turbofish on the
//!   type segment, not the method) doesn't over-approximate.
//! - Method-call (`s.method::<Session>()`): turbofish lives directly
//!   on `ExprMethodCall.turbofish`.
//!
//! Two semantically distinct rules — kept as two helpers so the call
//! sites compose them explicitly:
//! - `turbofish_substitute(inferred, …)` — applied by BOTH call shapes
//!   when the index returned a `GenericParamBound` (bare fn-generic-
//!   param ident return); the turbofish substitution is strictly more
//!   specific than the param's bound and wins. For any other shape
//!   (concrete `Path`, `TraitBound` from `impl Trait` / `dyn Trait`,
//!   wrappers, `Opaque`), returns the input unchanged — the turbofish
//!   args don't substitute those return positions.
//! - `turbofish_fallback(…)` — free-fn-only fallback when the index
//!   misses entirely (`external_fn::<X>()` patterns). Method calls
//!   deliberately do NOT use this — an unindexed method on an Opaque
//!   receiver gives no signal that the turbofish substitution is
//!   meaningful, and firing it would synthesise false
//!   `Session::method()` edges whenever the receiver type is unknown.

use super::super::canonical::CanonicalType;
use super::super::resolve::{resolve_type, ResolveContext};
use super::super::self_subst::substitute_bare_self;
use super::InferContext;

// qual:api
/// Apply turbofish substitution when the inferred return is a
/// `GenericParamBound` (bare fn-generic-param ident return). Shared by
/// `infer_call` (path-style) and `infer_method_call` (method-style):
/// both call shapes have the symmetric rule "the caller named the
/// concrete type via turbofish; that's strictly more specific than
/// the param's bound." For any other inferred shape (`Path`,
/// `TraitBound` from `impl Trait` / `dyn Trait`, wrappers, `Opaque`),
/// returns the input unchanged — the turbofish args don't substitute
/// those return positions. Operation.
pub(super) fn turbofish_substitute(
    inferred: CanonicalType,
    turbofish: Option<&syn::AngleBracketedGenericArguments>,
    ctx: &InferContext<'_>,
) -> CanonicalType {
    if matches!(inferred, CanonicalType::GenericParamBound(_)) {
        if let Some(tf) = turbofish.and_then(|args| resolve_first_type_arg(args, ctx)) {
            return tf;
        }
    }
    inferred
}

// qual:api
/// Free-fn-only fallback: when the workspace index doesn't have a
/// concrete return for a single-segment generic path call, take the
/// turbofish's first type arg as a best-effort guess (`external_fn::<X>()`
/// patterns). Method calls deliberately do NOT use this fallback — an
/// unindexed method on an Opaque receiver gives no signal that the
/// turbofish substitution is meaningful, and firing it would synthesise
/// false `Session::method()` edges on `s.method::<Session>()` whenever
/// `s`'s type is unknown. Operation.
pub(super) fn turbofish_fallback(
    turbofish: Option<&syn::AngleBracketedGenericArguments>,
    ctx: &InferContext<'_>,
) -> Option<CanonicalType> {
    turbofish.and_then(|args| resolve_first_type_arg(args, ctx))
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
