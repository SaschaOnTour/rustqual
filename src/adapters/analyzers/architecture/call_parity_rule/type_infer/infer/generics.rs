//! Stage 2 generic-fn handling — turbofish as return-type override.
//!
//! When a single-ident generic fn is called with a turbofish
//! (`get::<Session>()`) and the workspace index has no concrete return
//! type for it (because the fn's signature returns a generic `T` that
//! collapses to `Opaque`), the turbofish's first type argument serves
//! as the inferred return type. Narrow by design — it only fires for
//! single-segment paths so `Vec::<u32>::new()` (where the turbofish
//! sits on the type segment, not the method) doesn't over-approximate.

use super::super::canonical::CanonicalType;
use super::super::resolve::{resolve_type, ResolveContext};
use super::InferContext;

/// Fallback after `fn_returns` / `method_returns` miss: use the
/// turbofish's first type argument as the return type. Returns `None`
/// for multi-segment paths, missing angle-bracketed args, or `Opaque`
/// turbofish types. Operation.
pub(super) fn turbofish_return_type(
    path: &syn::Path,
    ctx: &InferContext<'_>,
) -> Option<CanonicalType> {
    if path.segments.len() != 1 {
        return None;
    }
    let only = &path.segments[0];
    let syn::PathArguments::AngleBracketed(ab) = &only.arguments else {
        return None;
    };
    let first_ty = ab.args.iter().find_map(|arg| match arg {
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
    };
    let resolved = resolve_type(first_ty, &rctx);
    if matches!(resolved, CanonicalType::Opaque) {
        return None;
    }
    Some(resolved)
}
