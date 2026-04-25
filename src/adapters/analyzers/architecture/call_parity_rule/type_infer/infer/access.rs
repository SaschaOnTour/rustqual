//! Inference for `Field`, `Try`, `Await`, `Cast`, and `Unary(Deref)`.
//!
//! These are the "structural" expressions — they don't introduce new
//! resolution but peel / project existing types:
//! - `base.field` → struct-field lookup on base's type
//! - `expr?` → `Result<T,_>` / `Option<T>` → `T`
//! - `expr.await` → `Future<Output=T>` → `T`
//! - `expr as T` → `T` (re-resolves the target type)
//! - `*expr` → same type as `expr` (deref is transparent for call graphs)

use super::super::canonical::CanonicalType;
use super::super::resolve::{resolve_type, ResolveContext};
use super::InferContext;

/// `base.field` — recurse on `base`, then look up the field in the
/// workspace index. Integration.
pub(super) fn infer_field(f: &syn::ExprField, ctx: &InferContext<'_>) -> Option<CanonicalType> {
    let base_type = super::infer_type(&f.base, ctx)?;
    let syn::Member::Named(ident) = &f.member else {
        return None;
    };
    let field_name = ident.to_string();
    lookup_field(&base_type, &field_name, ctx)
}

/// Only `Path` receiver types hit the struct-field index. Stdlib
/// wrappers don't have user-defined fields. Operation.
fn lookup_field(ty: &CanonicalType, field: &str, ctx: &InferContext<'_>) -> Option<CanonicalType> {
    match ty {
        CanonicalType::Path(segs) => {
            let key = segs.join("::");
            ctx.workspace.struct_field(&key, field).cloned()
        }
        _ => None,
    }
}

/// `expr?` — unwrap `Result<T,_>` or `Option<T>` to `T`. Operation:
/// delegate to `CanonicalType::happy_inner` (which matches both wrappers
/// and, permissively, `Future` — harmless since `?` on Future is a
/// compile error anyway).
pub(super) fn infer_try(t: &syn::ExprTry, ctx: &InferContext<'_>) -> Option<CanonicalType> {
    let inner = super::infer_type(&t.expr, ctx)?;
    inner.happy_inner().cloned()
}

/// `expr.await` — unwrap `Future<Output=T>` to `T` only. Operation.
pub(super) fn infer_await(a: &syn::ExprAwait, ctx: &InferContext<'_>) -> Option<CanonicalType> {
    let inner = super::infer_type(&a.base, ctx)?;
    match inner {
        CanonicalType::Future(t) => Some(*t),
        _ => None,
    }
}

/// `expr as T` — re-resolve the target type. The source expression's
/// type is irrelevant (conversion semantics are beyond our scope).
/// Operation: delegate to `resolve_type`.
pub(super) fn infer_cast(c: &syn::ExprCast, ctx: &InferContext<'_>) -> Option<CanonicalType> {
    let rctx = ResolveContext {
        alias_map: ctx.alias_map,
        local_symbols: ctx.local_symbols,
        crate_root_modules: ctx.crate_root_modules,
        importing_file: ctx.importing_file,
        type_aliases: Some(&ctx.workspace.type_aliases),
        transparent_wrappers: Some(&ctx.workspace.transparent_wrappers),
        local_decl_scopes: None,
        mod_stack: &[],
    };
    let ty = resolve_type(&c.ty, &rctx);
    if ty.is_opaque() {
        None
    } else {
        Some(ty)
    }
}

/// `*expr` — transparent for call-graph purposes (auto-deref produces
/// the same method table access). Other unary operators (`!x`, `-x`)
/// produce primitives we don't track — return `None`. Operation.
pub(super) fn infer_unary(u: &syn::ExprUnary, ctx: &InferContext<'_>) -> Option<CanonicalType> {
    if !matches!(u.op, syn::UnOp::Deref(_)) {
        return None;
    }
    super::infer_type(&u.expr, ctx)
}
