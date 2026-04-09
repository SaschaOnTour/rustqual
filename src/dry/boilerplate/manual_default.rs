use syn::spanned::Spanned;

use super::{
    is_default_value_expr, self_type_of, single_return_expr, trait_name_of, BoilerplateFind,
};
use crate::config::sections::BoilerplateConfig;

/// Detect `impl Default` where all fields use default values.
/// Operation: AST pattern matching logic; helper calls in closures.
pub(super) fn check_manual_default(
    parsed: &[(String, String, syn::File)],
    config: &BoilerplateConfig,
) -> Vec<BoilerplateFind> {
    pattern_guard!("BP-005", config);
    parsed
        .iter()
        .flat_map(|(file, _, syntax)| {
            syntax.items.iter().filter_map({
                let file = file.clone();
                move |item| {
                    let imp = if let syn::Item::Impl(imp) = item {
                        imp
                    } else {
                        return None;
                    };
                    if trait_name_of(imp).as_deref() != Some("Default") {
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
                    if methods.len() != 1 || methods[0].sig.ident != "default" {
                        return None;
                    }
                    let expr = single_return_expr(&methods[0].block)?;
                    // Must be a struct literal with all default-value fields
                    let all_defaults = if let syn::Expr::Struct(s) = expr {
                        s.rest.is_none()
                            && !s.fields.is_empty()
                            && s.fields.iter().all(|f| is_default_value_expr(&f.expr))
                    } else {
                        false
                    };
                    if !all_defaults {
                        return None;
                    }
                    Some(BoilerplateFind {
                        pattern_id: "BP-005".to_string(),
                        file: file.clone(),
                        line: imp.self_ty.span().start().line,
                        struct_name: self_type_of(imp),
                        description:
                            "Manual Default implementation where all fields use default values"
                                .to_string(),
                        suggestion: "Consider using #[derive(Default)]".to_string(),
                        suppressed: false,
                    })
                }
            })
        })
        .collect()
}
