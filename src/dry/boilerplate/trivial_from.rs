use syn::spanned::Spanned;

use super::{self_type_of, single_return_expr, trait_name_of, BoilerplateFind};
use crate::config::sections::BoilerplateConfig;

/// Detect trivial `impl From<T> for U` that just wraps a value.
/// Operation: AST pattern matching logic; helper calls in closures.
// qual:allow(complexity) reason: "AST pattern matching with nested closures"
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
            syntax.items.iter().filter_map({
                let file = file.clone();
                let suggest = suggest.to_string();
                move |item| {
                    let imp = if let syn::Item::Impl(imp) = item {
                        imp
                    } else {
                        return None;
                    };
                    if trait_name_of(imp).as_deref() != Some("From") {
                        return None;
                    }
                    let methods: Vec<_> = imp
                        .items
                        .iter()
                        .filter_map(|i| {
                            if let syn::ImplItem::Fn(m) = i {
                                Some(m)
                            } else {
                                None
                            }
                        })
                        .collect();
                    if methods.len() != 1 || methods[0].sig.ident != "from" {
                        return None;
                    }
                    let expr = single_return_expr(&methods[0].block)?;
                    // Trivial: constructor call with simple args, or struct literal with simple fields
                    let is_trivial = match expr {
                        syn::Expr::Call(c) => {
                            c.args.iter().all(|a| matches!(a, syn::Expr::Path(_)))
                        }
                        syn::Expr::Struct(s) => {
                            s.rest.is_none()
                                && s.fields
                                    .iter()
                                    .all(|f| matches!(f.expr, syn::Expr::Path(_)))
                        }
                        _ => false,
                    };
                    if !is_trivial {
                        return None;
                    }
                    Some(BoilerplateFind {
                        pattern_id: "BP-001".to_string(),
                        file: file.clone(),
                        line: imp.self_ty.span().start().line,
                        struct_name: self_type_of(imp),
                        description: "Trivial From implementation that just wraps a value"
                            .to_string(),
                        suggestion: suggest.clone(),
                        suppressed: false,
                    })
                }
            })
        })
        .collect()
}
