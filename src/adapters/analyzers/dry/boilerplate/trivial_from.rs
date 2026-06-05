use syn::spanned::Spanned;

use super::{self_type_of, single_return_expr, trait_name_of, BoilerplateFind};
use crate::config::sections::BoilerplateConfig;

/// Detect trivial `impl From<T> for U` that just wraps a value.
/// Operation: per-file item scan, detection delegated to a helper in closures.
pub(super) fn check_trivial_from(
    parsed: &[(String, String, syn::File)],
    config: &BoilerplateConfig,
) -> Vec<BoilerplateFind> {
    pattern_guard!("BP-001", config);
    let suggest = if config.suggest_crates {
        "Consider using derive_more::From"
    } else {
        "Consider using a derive macro for trivial conversions"
    };
    parsed
        .iter()
        .flat_map(|(file, _, syntax)| {
            syntax
                .items
                .iter()
                .filter_map(|item| trivial_from_find(item, file, suggest))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Build a BP-001 find for a single trivial `impl From` item, or `None`.
/// Operation: AST pattern matching; helper calls in closures.
fn trivial_from_find(item: &syn::Item, file: &str, suggest: &str) -> Option<BoilerplateFind> {
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
            s.rest.is_none() && s.fields.iter().all(|f| matches!(f.expr, syn::Expr::Path(_)))
        }
        _ => false,
    }
}
