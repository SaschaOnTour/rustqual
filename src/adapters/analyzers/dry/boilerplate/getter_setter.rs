use super::{is_self_field_access, single_return_expr, BoilerplateFind};
use crate::config::sections::BoilerplateConfig;

/// Minimum getter/setter methods to flag on one struct.
const MIN_GETTER_SETTER_COUNT: usize = 3;

// qual:allow(complexity) reason: "getter/setter detection requires nested AST inspection"
/// Detect structs with many trivial getter/setter methods.
/// Operation: per-impl counting logic; helper calls in closures.
pub(super) fn check_manual_getter_setter(
    parsed: &[(String, String, syn::File)],
    config: &BoilerplateConfig,
) -> Vec<BoilerplateFind> {
    pattern_guard!("BP-003", config);
    let mut findings = Vec::new();
    for (file, _, syntax) in parsed {
        for item in &syntax.items {
            let imp = if let syn::Item::Impl(imp) = item {
                imp
            } else {
                continue;
            };
            // Skip trait impls
            if imp.trait_.is_some() {
                continue;
            }
            let getter_methods: Vec<&syn::ImplItemFn> = imp
                .items
                .iter()
                .filter_map(|i| {
                    if let syn::ImplItem::Fn(m) = i {
                        let is_getter = m.block.stmts.len() == 1
                            && m.sig.inputs.len() == 1
                            && {
                                if let Some(expr) = single_return_expr(&m.block) {
                                    is_self_field_access(expr)
                                        || matches!(expr, syn::Expr::Reference(r) if is_self_field_access(&r.expr))
                                } else {
                                    false
                                }
                            };
                        let is_setter = m.block.stmts.len() == 1
                            && m.sig.inputs.len() == 2
                            && matches!(
                                &m.block.stmts[0],
                                syn::Stmt::Expr(syn::Expr::Assign(a), Some(_))
                                    if is_self_field_access(&a.left)
                            );
                        if is_getter || is_setter { Some(m) } else { None }
                    } else {
                        None
                    }
                })
                .collect();
            if getter_methods.len() >= MIN_GETTER_SETTER_COUNT {
                let struct_name = if let syn::Type::Path(tp) = &*imp.self_ty {
                    tp.path.segments.last().map(|s| s.ident.to_string())
                } else {
                    None
                };
                getter_methods.iter().for_each(|m| {
                    findings.push(BoilerplateFind {
                        pattern_id: "BP-003".to_string(),
                        file: file.clone(),
                        line: m.sig.ident.span().start().line,
                        struct_name: struct_name.clone(),
                        description:
                            "trivial getter/setter — consider field visibility or accessor macro"
                                .to_string(),
                        suggestion:
                            "Consider making fields pub or using a getter/setter derive macro"
                                .to_string(),
                        suppressed: false,
                    });
                });
            }
        }
    }
    findings
}
