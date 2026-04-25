//! Bare-`Self` substitution for `syn::Type`.
//!
//! Used by both the workspace-index method-return collector and the
//! call collector to rewrite `Self` to the enclosing impl's canonical
//! segments before handing off to `resolve_type`. Without this,
//! wrapper return types (`Result<Self, E>`, `Option<Self>`) and
//! impl-body declarations (`fn merge(&self, other: Self)`,
//! `let other: Self = …`) collapse the inner `Self` to `Opaque`
//! because the resolver itself has no impl context.
//!
//! Multi-segment paths like `Self::Output` are left untouched —
//! associated-type resolution is out of scope and would need its own
//! type-bound walker.

use syn::visit_mut::VisitMut;

// qual:api
/// Clone `ty` and rewrite every bare `Self` ident path to `impl_segs`.
/// Returns the input untouched when `impl_segs.join("::")` doesn't
/// parse as a path (defensive — real impl segments always do).
pub(crate) fn substitute_bare_self(ty: &syn::Type, impl_segs: &[String]) -> syn::Type {
    let mut out = ty.clone();
    let Ok(replacement) = syn::parse_str::<syn::Path>(&impl_segs.join("::")) else {
        return out;
    };
    SelfPathRewriter { replacement }.visit_type_mut(&mut out);
    out
}

/// `VisitMut` adapter that replaces each `Type::Path` whose path is a
/// single bare `Self` with the impl's canonical path. Multi-segment
/// `Self::Output` is intentionally left alone.
struct SelfPathRewriter {
    replacement: syn::Path,
}

impl VisitMut for SelfPathRewriter {
    fn visit_type_mut(&mut self, ty: &mut syn::Type) {
        if let syn::Type::Path(tp) = ty {
            if is_bare_self_path(tp) {
                *ty = syn::Type::Path(syn::TypePath {
                    qself: None,
                    path: self.replacement.clone(),
                });
                return;
            }
        }
        syn::visit_mut::visit_type_mut(self, ty);
    }
}

/// True when `tp` is `Self` with no qself, no further segments, and
/// no path arguments — the only shape that maps unambiguously to the
/// enclosing impl's self-type. Operation.
fn is_bare_self_path(tp: &syn::TypePath) -> bool {
    if tp.qself.is_some() || tp.path.segments.len() != 1 {
        return false;
    }
    let seg = &tp.path.segments[0];
    seg.ident == "Self" && matches!(seg.arguments, syn::PathArguments::None)
}
