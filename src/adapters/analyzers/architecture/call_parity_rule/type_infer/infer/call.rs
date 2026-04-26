//! Inference for `Path`, `Call`, and `MethodCall` expressions.
//!
//! These three cover the bulk of call-graph-relevant resolution:
//! - A bare `Expr::Path(ident)` inside an expression position refers to a
//!   local variable — we look it up in the scoped bindings.
//! - `Expr::Call { func: Path(…) }` is a free-fn or associated-fn invocation;
//!   its inferred type is the return type stored in the workspace index.
//! - `Expr::MethodCall` recursively infers the receiver type, then looks up
//!   `(receiver_canonical, method)` in the workspace index.

use super::super::canonical::CanonicalType;
use super::generics::turbofish_return_type;
use super::InferContext;
use crate::adapters::analyzers::architecture::call_parity_rule::bindings::{
    canonicalise_type_segments_in_scope, CanonScope,
};

/// A bare `Expr::Path` in expression position is always either a local
/// variable or a const/static ref. Stage 1 resolves only locals.
/// Operation: lookup + clone.
pub(super) fn infer_path_expr(p: &syn::ExprPath, ctx: &InferContext<'_>) -> Option<CanonicalType> {
    if p.path.segments.len() != 1 {
        return None;
    }
    let ident = p.path.segments[0].ident.to_string();
    ctx.bindings.lookup(&ident)
}

/// A call like `foo()` or `T::ctor(...)` or `crate::path::fn(...)`.
/// Resolve the func path to its canonical form, try the workspace
/// index in two ways (fn-style first, method-style as fallback), and
/// finally — for single-ident generic fns called with a turbofish
/// (`get::<Session>()`) — use the turbofish type argument as the
/// inferred return type. Stage 2 feature.
/// Integration: delegates to the three lookup helpers.
pub(super) fn infer_call(call: &syn::ExprCall, ctx: &InferContext<'_>) -> Option<CanonicalType> {
    let syn::Expr::Path(p) = call.func.as_ref() else {
        return None;
    };
    let segs = path_segments(p);
    if let Some(t) = infer_call_from_segments(&segs, ctx) {
        return Some(t);
    }
    turbofish_return_type(&p.path, ctx)
}

/// Receiver-type-driven method resolution: `expr.method(…)`. Recurses
/// into `expr` via the top-level `infer_type`, then looks the result up
/// in the workspace index. Integration.
pub(super) fn infer_method_call(
    m: &syn::ExprMethodCall,
    ctx: &InferContext<'_>,
) -> Option<CanonicalType> {
    let receiver_type = super::infer_type(&m.receiver, ctx)?;
    let method = m.method.to_string();
    lookup_method_on_type(&receiver_type, &method, ctx)
}

/// Try `fn_returns` first, fall back to `method_returns` if path has
/// at least two segments. Operation: delegation via `.or_else`.
fn infer_call_from_segments(segs: &[String], ctx: &InferContext<'_>) -> Option<CanonicalType> {
    if segs.is_empty() {
        return None;
    }
    try_fn_return(segs, ctx).or_else(|| try_method_return(segs, ctx))
}

/// Canonicalise the full path and probe `fn_returns`. Operation.
fn try_fn_return(segs: &[String], ctx: &InferContext<'_>) -> Option<CanonicalType> {
    let full = canonicalise_call_path(segs, ctx)?;
    let key = full.join("::");
    ctx.workspace.fn_return(&key).cloned()
}

/// Split the last segment off as method name, canonicalise the prefix
/// as a type path, and probe `method_returns`. Operation.
fn try_method_return(segs: &[String], ctx: &InferContext<'_>) -> Option<CanonicalType> {
    if segs.len() < 2 {
        return None;
    }
    let method = segs.last()?;
    let type_segs = &segs[..segs.len() - 1];
    let type_full = canonicalise_call_path(type_segs, ctx)?;
    let key = type_full.join("::");
    ctx.workspace.method_return(&key, method).cloned()
}

/// Method lookup keyed on the receiver's canonical type. `Path` hits
/// the user-defined workspace index; stdlib wrappers
/// (`Result`/`Option`/`Future`) go through the combinator table;
/// `TraitBound` (Stage 2) picks the first matching trait-impl's
/// return type (all impls share the trait's signature).
/// `Slice`/`Map`/`Opaque` stay unresolved.
/// Operation: dispatch over wrapper kind.
fn lookup_method_on_type(
    ty: &CanonicalType,
    method: &str,
    ctx: &InferContext<'_>,
) -> Option<CanonicalType> {
    match ty {
        CanonicalType::Path(segs) => {
            let key = segs.join("::");
            ctx.workspace.method_return(&key, method).cloned()
        }
        CanonicalType::Result(_) | CanonicalType::Option(_) | CanonicalType::Future(_) => {
            super::super::combinators::combinator_return(ty, method)
        }
        CanonicalType::TraitBound(segs) => lookup_trait_method_return(segs, method, ctx),
        _ => None,
    }
}

/// For a `dyn Trait` receiver, find the first workspace impl that has
/// the method in question and use its return type. Valid Rust guarantees
/// all impls share the trait's method signature, so the first one wins.
/// Operation: index probe + lookup.
fn lookup_trait_method_return(
    trait_segs: &[String],
    method: &str,
    ctx: &InferContext<'_>,
) -> Option<CanonicalType> {
    let trait_canonical = trait_segs.join("::");
    if !ctx.workspace.trait_has_method(&trait_canonical, method) {
        return None;
    }
    ctx.workspace
        .impls_of_trait(&trait_canonical)
        .iter()
        .find_map(|impl_type| ctx.workspace.method_return(impl_type, method).cloned())
}

/// Canonicalise a path's segments for lookup, with `Self`-substitution
/// applied before the generic pipeline. Operation: substitution +
/// delegate.
fn canonicalise_call_path(segs: &[String], ctx: &InferContext<'_>) -> Option<Vec<String>> {
    if segs.is_empty() {
        return None;
    }
    let expanded = substitute_self(segs, ctx.self_type.as_ref())?;
    canonicalise_type_segments_in_scope(
        &expanded,
        &CanonScope {
            file: ctx.file,
            mod_stack: ctx.mod_stack,
        },
    )
}

/// If the first segment is `Self`, replace it with the impl's self-type
/// canonical segments. Returns `None` when `Self` appears without a
/// self-type context (shouldn't happen syntactically, but keeps the
/// resolver honest). Operation.
fn substitute_self(segs: &[String], self_type: Option<&Vec<String>>) -> Option<Vec<String>> {
    if segs.first().map(String::as_str) != Some("Self") {
        return Some(segs.to_vec());
    }
    let self_segs = self_type?;
    let mut out: Vec<String> = self_segs.clone();
    out.extend_from_slice(&segs[1..]);
    Some(out)
}

/// Flatten a `syn::ExprPath` to its segment idents. Operation.
fn path_segments(p: &syn::ExprPath) -> Vec<String> {
    p.path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect()
}
