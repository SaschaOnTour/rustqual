use syn::spanned::Spanned;

use super::BoilerplateFind;
use crate::config::sections::BoilerplateConfig;

/// Minimum From impls to flag as error enum boilerplate.
const MIN_FROM_IMPLS_FOR_ERROR: usize = 3;

// qual:allow(complexity) reason: "From impl grouping requires nested AST inspection"
/// Detect enums with multiple trivial `impl From<T>` for wrapping errors.
/// Operation: grouping + counting logic; helper calls in closures.
pub(super) fn check_error_enum_boilerplate(
    parsed: &[(String, String, syn::File)],
    config: &BoilerplateConfig,
) -> Vec<BoilerplateFind> {
    pattern_guard!("BP-007", config);
    let suggest = if config.suggest_crates {
        "Consider using thiserror to derive From implementations"
    } else {
        "Consider using a derive macro to generate From implementations for error variants"
    };
    let mut findings = Vec::new();
    for (file, _, syntax) in parsed {
        // Collect all trivial From<T> impls grouped by target type
        let mut from_counts: std::collections::HashMap<String, (usize, usize)> =
            std::collections::HashMap::new();
        for item in &syntax.items {
            let imp = if let syn::Item::Impl(imp) = item {
                imp
            } else {
                continue;
            };
            let is_from = imp.trait_.as_ref().is_some_and(|(_, path, _)| {
                path.segments.last().is_some_and(|s| s.ident == "From")
            });
            if !is_from {
                continue;
            }
            let self_name = if let syn::Type::Path(tp) = &*imp.self_ty {
                if let Some(seg) = tp.path.segments.last() {
                    seg.ident.to_string()
                } else {
                    continue;
                }
            } else {
                continue;
            };
            // Check if single method with simple wrapping body
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
            if methods.len() == 1 && methods[0].sig.ident == "from" {
                let has_single_expr = methods[0].block.stmts.len() == 1
                    && matches!(&methods[0].block.stmts[0], syn::Stmt::Expr(_, None));
                if has_single_expr {
                    let entry = from_counts
                        .entry(self_name)
                        .or_insert((0, imp.self_ty.span().start().line));
                    entry.0 += 1;
                }
            }
        }
        for (type_name, (count, line)) in &from_counts {
            if *count >= MIN_FROM_IMPLS_FOR_ERROR {
                findings.push(BoilerplateFind {
                    pattern_id: "BP-007".to_string(),
                    file: file.clone(),
                    line: *line,
                    struct_name: Some(type_name.clone()),
                    description: format!(
                        "{count} trivial From impls for {type_name} — error enum boilerplate"
                    ),
                    suggestion: suggest.to_string(),
                    suppressed: false,
                });
            }
        }
    }
    findings
}
