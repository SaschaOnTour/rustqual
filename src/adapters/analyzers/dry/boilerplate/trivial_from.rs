use syn::spanned::Spanned;

use super::{self_type_of, single_return_expr, trait_name_of, BoilerplateFind};
use crate::config::sections::BoilerplateConfig;

/// Detect trivial `impl From<T> for U` that just wraps a value.
/// Integration: delegates the per-item scan to the shared `scan_items`.
pub(super) fn check_trivial_from(
    parsed: &[(String, String, syn::File)],
    config: &BoilerplateConfig,
) -> Vec<BoilerplateFind> {
    super::scan_items(parsed, config, "BP-001", |item, file| {
        trivial_from_find(item, file, config)
    })
}

/// Build a BP-001 find for a single trivial `impl From` item, or `None`.
/// Operation: AST pattern matching; helper calls in closures.
fn trivial_from_find(
    item: &syn::Item,
    file: &str,
    config: &BoilerplateConfig,
) -> Option<BoilerplateFind> {
    let syn::Item::Impl(imp) = item else {
        return None;
    };
    if trait_name_of(imp).as_deref() != Some("From") {
        return None;
    }
    let methods: Vec<_> = imp
        .items
        .iter()
        .filter_map(|i| match i {
            syn::ImplItem::Fn(m) => Some(m),
            _ => None,
        })
        .collect();
    if methods.len() != 1 || methods[0].sig.ident != "from" {
        return None;
    }
    let expr = single_return_expr(&methods[0].block)?;
    if !is_trivial_wrap(expr) {
        return None;
    }
    let suggest = if config.suggest_crates {
        "Consider using derive_more::From"
    } else {
        "Consider using a derive macro for trivial conversions"
    };
    Some(BoilerplateFind {
        pattern_id: "BP-001".to_string(),
        file: file.to_string(),
        line: imp.self_ty.span().start().line,
        struct_name: self_type_of(imp),
        description: "Trivial From implementation that just wraps a value".to_string(),
        suggestion: suggest.to_string(),
        suppressed: false,
    })
}

/// Whether a `from` body is a trivial wrap: a constructor call with only path
/// args, or a struct literal with only path field values.
/// Operation: expression match, no own calls.
fn is_trivial_wrap(expr: &syn::Expr) -> bool {
    match expr {
        syn::Expr::Call(c) => c.args.iter().all(|a| matches!(a, syn::Expr::Path(_))),
        syn::Expr::Struct(s) => {
            s.rest.is_none()
                && s.fields
                    .iter()
                    .all(|f| matches!(f.expr, syn::Expr::Path(_)))
        }
        _ => false,
    }
}
