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
/// `GenericParamBound` (bare callee-generic-param ident return).
/// Recurses through `Result` / `Option` / `Future` wrappers so a
/// `Result<Q, E>` return with turbofish `<Session>` substitutes the
/// inner `Q → Session` and rewraps as `Result(Session)` — `.unwrap()`
/// then yields Session and `.diff()` resolves correctly. The
/// substitution uses the param's `turbofish_index` to pick the right
/// turbofish arg (handles `fn get<A, Q: T>() -> Q` where Q is at
/// position 1, not 0). For impl-level params merged into a method
/// (turbofish_index = None) or any non-substitutable shape (`Path`,
/// `TraitBound` from `impl Trait` / `dyn Trait`, `Opaque`), returns
/// the input unchanged. Integration.
pub(super) fn turbofish_substitute(
    inferred: CanonicalType,
    turbofish: Option<&syn::AngleBracketedGenericArguments>,
    ctx: &InferContext<'_>,
) -> CanonicalType {
    match inferred {
        CanonicalType::GenericParamBound {
            bounds,
            turbofish_index: Some(idx),
        } => turbofish
            .and_then(|args| resolve_type_arg_at(args, idx, ctx))
            .unwrap_or(CanonicalType::GenericParamBound {
                bounds,
                turbofish_index: Some(idx),
            }),
        // Recurse into wrappers so generic-param returns inside
        // `Result<Q, _>` / `Option<Q>` / `Future<Output = Q>` get
        // substituted before `.unwrap()` / `.await` peel the wrapper.
        CanonicalType::Result(inner) => {
            CanonicalType::Result(Box::new(turbofish_substitute(*inner, turbofish, ctx)))
        }
        CanonicalType::Option(inner) => {
            CanonicalType::Option(Box::new(turbofish_substitute(*inner, turbofish, ctx)))
        }
        CanonicalType::Future(inner) => {
            CanonicalType::Future(Box::new(turbofish_substitute(*inner, turbofish, ctx)))
        }
        other => other,
    }
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
    turbofish.and_then(|args| resolve_type_arg_at(args, 0, ctx))
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

/// Resolve the type argument at position `idx` inside an angle-bracketed
/// args list (skipping lifetime / const args; `idx` counts only type
/// args). Returns `None` if the index is out of bounds or the type
/// collapses to `Opaque`. Operation.
fn resolve_type_arg_at(
    args: &syn::AngleBracketedGenericArguments,
    idx: usize,
    ctx: &InferContext<'_>,
) -> Option<CanonicalType> {
    let type_arg = args
        .args
        .iter()
        .filter_map(|arg| match arg {
            syn::GenericArgument::Type(t) => Some(t),
            _ => None,
        })
        .nth(idx)?;
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
        Some(impl_segs) => resolve_type(&substitute_bare_self(type_arg, impl_segs), &rctx),
        None => resolve_type(type_arg, &rctx),
    };
    if matches!(resolved, CanonicalType::Opaque) {
        return None;
    }
    Some(resolved)
}
