use syn::spanned::Spanned;

use super::{self_type_of, single_return_expr, trait_name_of, BoilerplateFind};
use crate::config::sections::BoilerplateConfig;

/// Check whether an expression is a `write!` or `writeln!` macro call.
/// Operation: AST pattern matching logic, no own calls.
fn is_write_macro(expr: &syn::Expr) -> bool {
    if let syn::Expr::Macro(m) = expr {
        m.mac
            .path
            .segments
            .last()
            .map(|s| s.ident == "write" || s.ident == "writeln")
            .unwrap_or(false)
    } else {
        false
    }
}

/// Detect trivial `impl Display` with a single write!/writeln! call.
/// Operation: AST pattern matching logic; helper calls in closures.
pub(super) fn check_trivial_display(
    parsed: &[(String, String, syn::File)],
    config: &BoilerplateConfig,
) -> Vec<BoilerplateFind> {
    pattern_guard!("BP-002", config);
    let suggest = if config.suggest_crates {
        "Consider using derive_more::Display"
    } else {
        "Consider using a derive macro for simple Display implementations"
    };
    let is_write = |e: &syn::Expr| is_write_macro(e);
    parsed
        .iter()
        .flat_map(|(file, _, syntax)| {
            syntax.items.iter().filter_map({
                let file = file.clone();
                let suggest = suggest.to_string();
                move |item| {
                    let syn::Item::Impl(imp) = item else {
                        return None;
                    };
                    if trait_name_of(imp)? != "Display" {
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
                    if methods.len() != 1 || methods[0].sig.ident != "fmt" {
                        return None;
                    }
                    let expr = single_return_expr(&methods[0].block)?;
                    if !is_write(expr) {
                        return None;
                    }
                    Some(BoilerplateFind {
                        pattern_id: "BP-002".to_string(),
                        file: file.clone(),
                        line: imp.self_ty.span().start().line,
                        struct_name: self_type_of(imp),
                        description: "Trivial Display implementation with a single write! call"
                            .to_string(),
                        suggestion: suggest.clone(),
                    })
                }
            })
        })
        .collect()
}
