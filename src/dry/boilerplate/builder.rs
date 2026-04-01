use syn::spanned::Spanned;

use super::{is_self_field_access, BoilerplateFind};
use crate::config::sections::BoilerplateConfig;

/// Minimum builder-style methods to flag on one struct.
const MIN_BUILDER_METHOD_COUNT: usize = 3;

/// Detect structs with many builder-style methods (set field + return self).
/// Operation: per-impl counting logic; helper calls in closures.
// qual:allow(complexity) reason: "builder pattern detection requires nested AST inspection"
pub(super) fn check_builder_boilerplate(
    parsed: &[(String, String, syn::File)],
    config: &BoilerplateConfig,
) -> Vec<BoilerplateFind> {
    pattern_guard!("BP-004", config);
    let suggest = if config.suggest_crates {
        "Consider using typed_builder or derive_builder"
    } else {
        "Consider using a builder derive macro to reduce repetition"
    };
    let mut findings = Vec::new();
    for (file, _, syntax) in parsed {
        for item in &syntax.items {
            let imp = if let syn::Item::Impl(imp) = item {
                imp
            } else {
                continue;
            };
            if imp.trait_.is_some() {
                continue;
            }
            let count = imp
                .items
                .iter()
                .filter(|i| {
                    if let syn::ImplItem::Fn(m) = i {
                        // Must return Self
                        let returns_self = if let syn::ReturnType::Type(_, ty) = &m.sig.output {
                            if let syn::Type::Path(tp) = &**ty {
                                tp.path.segments.last().is_some_and(|s| s.ident == "Self")
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        if !returns_self || m.block.stmts.len() != 2 {
                            return false;
                        }
                        // First stmt: self.field = value;
                        let has_assign = matches!(
                            &m.block.stmts[0],
                            syn::Stmt::Expr(syn::Expr::Assign(a), Some(_))
                                if is_self_field_access(&a.left)
                        );
                        // Second stmt: self (return)
                        let returns_self_val = matches!(
                            &m.block.stmts[1],
                            syn::Stmt::Expr(syn::Expr::Path(p), None)
                                if p.path.segments.last().is_some_and(|s| s.ident == "self")
                        );
                        has_assign && returns_self_val
                    } else {
                        false
                    }
                })
                .count();
            if count >= MIN_BUILDER_METHOD_COUNT {
                let struct_name = if let syn::Type::Path(tp) = &*imp.self_ty {
                    tp.path.segments.last().map(|s| s.ident.to_string())
                } else {
                    None
                };
                findings.push(BoilerplateFind {
                    pattern_id: "BP-004".to_string(),
                    file: file.clone(),
                    line: imp.self_ty.span().start().line,
                    struct_name,
                    description: format!(
                        "{count} builder-style methods with repetitive set-and-return pattern"
                    ),
                    suggestion: suggest.to_string(),
                });
            }
        }
    }
    findings
}
