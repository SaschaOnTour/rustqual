use super::{is_repetitive_enum_mapping, BoilerplateFind};
use crate::config::sections::BoilerplateConfig;

/// Minimum match arms for repetitive match detection.
const MIN_REPETITIVE_MATCH_ARMS: usize = 4;

/// Function names that are standard conversion patterns — not boilerplate.
/// Repetitive match arms in these functions map inputs to outputs by design.
const CONVERSION_FN_NAMES: &[&str] = &["from_str", "from_str_opt", "from", "try_from", "fmt"];

/// Detect match expressions with many arms that all follow the same
/// pattern (enum-to-enum mapping).
/// Operation: body inspection logic; helper calls in closures.
// qual:allow(complexity) reason: "match expression detection with closure-based body check"
pub(super) fn check_repetitive_match(
    parsed: &[(String, String, syn::File)],
    config: &BoilerplateConfig,
) -> Vec<BoilerplateFind> {
    pattern_guard!("BP-006", config);
    let suggest = if config.suggest_crates {
        "Consider using strum or a conversion derive macro"
    } else {
        "Consider using a derive macro or Into/From trait for enum mapping"
    };
    let mut findings = Vec::new();
    for (file, _, syntax) in parsed {
        let check_body = |block: &syn::Block, line: usize| {
            for stmt in &block.stmts {
                let match_expr = match stmt {
                    syn::Stmt::Expr(syn::Expr::Match(m), _) => m,
                    syn::Stmt::Local(local) => {
                        if let Some(init) = &local.init {
                            if let syn::Expr::Match(m) = &*init.expr {
                                m
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };
                // Skip tuple scrutinees (e.g. `match (a, b) { ... }`)
                if matches!(&*match_expr.expr, syn::Expr::Tuple(_)) {
                    continue;
                }
                if match_expr.arms.len() >= MIN_REPETITIVE_MATCH_ARMS
                    && is_repetitive_enum_mapping(&match_expr.arms)
                {
                    return Some(BoilerplateFind {
                        pattern_id: "BP-006".to_string(),
                        file: file.clone(),
                        line,
                        struct_name: None,
                        description: format!(
                            "Match with {} arms all mapping variants — may be replaceable with a derive",
                            match_expr.arms.len()
                        ),
                        suggestion: suggest.to_string(),
                    });
                }
            }
            None
        };
        for item in &syntax.items {
            match item {
                syn::Item::Fn(f) => {
                    let fn_name = f.sig.ident.to_string();
                    if CONVERSION_FN_NAMES.contains(&fn_name.as_str()) {
                        continue;
                    }
                    if let Some(finding) = check_body(&f.block, f.sig.ident.span().start().line) {
                        findings.push(finding);
                    }
                }
                syn::Item::Impl(imp) => {
                    for sub in &imp.items {
                        if let syn::ImplItem::Fn(m) = sub {
                            let fn_name = m.sig.ident.to_string();
                            if CONVERSION_FN_NAMES.contains(&fn_name.as_str()) {
                                continue;
                            }
                            if let Some(finding) =
                                check_body(&m.block, m.sig.ident.span().start().line)
                            {
                                findings.push(finding);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    findings
}
