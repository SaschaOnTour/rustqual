use syn::spanned::Spanned;

use super::{is_self_field_access, self_type_of, BoilerplateFind};
use crate::config::sections::BoilerplateConfig;

/// Minimum builder-style methods to flag on one struct.
const MIN_BUILDER_METHOD_COUNT: usize = 3;

/// Detect structs with many builder-style methods (set field + return self).
/// Integration: delegates the per-item scan to the shared `scan_items`.
pub(super) fn check_builder_boilerplate(
    parsed: &[(String, String, syn::File)],
    config: &BoilerplateConfig,
) -> Vec<BoilerplateFind> {
    super::scan_items(parsed, config, "BP-004", |item, file| {
        builder_find(item, file, config)
    })
}

/// BP-004 find for an item: a non-trait impl with at least
/// `MIN_BUILDER_METHOD_COUNT` builder-style methods.
/// Operation: method count + find building via closures.
fn builder_find(
    item: &syn::Item,
    file: &str,
    config: &BoilerplateConfig,
) -> Option<BoilerplateFind> {
    let syn::Item::Impl(imp) = item else {
        return None;
    };
    if imp.trait_.is_some() {
        return None;
    }
    let count = imp.items.iter().filter(|i| is_builder_method(i)).count();
    if count < MIN_BUILDER_METHOD_COUNT {
        return None;
    }
    let suggest = if config.suggest_crates {
        "Consider using typed_builder or derive_builder"
    } else {
        "Consider using a builder derive macro to reduce repetition"
    };
    Some(BoilerplateFind {
        pattern_id: "BP-004".to_string(),
        file: file.to_string(),
        line: imp.self_ty.span().start().line,
        struct_name: self_type_of(imp),
        description: format!(
            "{count} builder-style methods with repetitive set-and-return pattern"
        ),
        suggestion: suggest.to_string(),
        suppressed: false,
    })
}

/// A builder-style method: returns `Self`, body is exactly `self.field = v;` then
/// `self`.
/// Operation: shape checks via helpers.
fn is_builder_method(item: &syn::ImplItem) -> bool {
    let syn::ImplItem::Fn(m) = item else {
        return false;
    };
    method_returns_self(&m.sig.output)
        && m.block.stmts.len() == 2
        && is_field_assign(&m.block.stmts[0])
        && is_self_expr(&m.block.stmts[1])
}

/// Whether a return type names `Self` directly.
/// Operation: nested type match, no own calls.
fn method_returns_self(output: &syn::ReturnType) -> bool {
    matches!(output, syn::ReturnType::Type(_, ty)
        if matches!(&**ty, syn::Type::Path(tp)
            if tp.path.segments.last().is_some_and(|s| s.ident == "Self")))
}

/// Whether a statement assigns to a `self.field`.
/// Operation: shape match, own call in guard.
fn is_field_assign(stmt: &syn::Stmt) -> bool {
    matches!(stmt, syn::Stmt::Expr(syn::Expr::Assign(a), Some(_)) if is_self_field_access(&a.left))
}

/// Whether a statement is the bare `self` return expression.
/// Operation: shape match, no own calls.
fn is_self_expr(stmt: &syn::Stmt) -> bool {
    matches!(stmt, syn::Stmt::Expr(syn::Expr::Path(p), None)
        if p.path.segments.last().is_some_and(|s| s.ident == "self"))
}
