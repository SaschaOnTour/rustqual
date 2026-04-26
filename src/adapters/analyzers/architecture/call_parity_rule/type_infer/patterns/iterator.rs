//! For-loop element-type extraction.
//!
//! `for pat in iter_expr { … }` binds `pat` against the element type of
//! `iter_expr`. Stage 1 recognises `Slice(T)` as `T` (covers `Vec<T>`,
//! `&[T]`, `[T; N]` via `resolve_type`'s normalisation). Other iterables
//! — `HashMap::iter()` yielding `(&K, &V)`, user iterators — resolve to
//! `Opaque`; the pattern still walks but binds yield `Opaque`-typed
//! names.

use super::super::canonical::CanonicalType;
use super::super::infer::InferContext;
use super::destructure::extract_bindings;

// qual:api
/// Extract bindings from a for-loop's `pat in iter_expr`, using the
/// element type derived from `iter_type`. Integration: element-type
/// derivation + delegate to the general pattern walker.
pub fn extract_for_bindings(
    pat: &syn::Pat,
    iter_type: &CanonicalType,
    ctx: &InferContext<'_>,
) -> Vec<(String, CanonicalType)> {
    let elem = element_type_of(iter_type);
    extract_bindings(pat, &elem, ctx)
}

/// Map an iterable's canonical type to its element type. `Slice(T) → T`;
/// everything else collapses to `Opaque`. Operation.
fn element_type_of(iter_type: &CanonicalType) -> CanonicalType {
    match iter_type {
        CanonicalType::Slice(inner) => (**inner).clone(),
        _ => CanonicalType::Opaque,
    }
}
