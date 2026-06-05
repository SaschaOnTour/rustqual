use super::{is_repetitive_enum_mapping, BoilerplateFind};
use crate::config::sections::BoilerplateConfig;

/// Minimum match arms for repetitive match detection.
const MIN_REPETITIVE_MATCH_ARMS: usize = 4;

/// Function names that are standard conversion patterns — not boilerplate.
/// Repetitive match arms in these functions map inputs to outputs by design.
const CONVERSION_FN_NAMES: &[&str] = &["from_str", "from_str_opt", "from", "try_from", "fmt"];

/// Detect match expressions with many arms that all follow the same
/// pattern (enum-to-enum mapping).
/// Operation: per-file fn scan, detection delegated to helpers in closures.
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
    parsed
        .iter()
        .flat_map(|(file, _, syntax)| {
            syntax
                .items
                .iter()
                .flat_map(super::item_fns)
                .filter(|(sig, _)| !is_conversion_fn(sig))
                .filter_map(|(sig, block)| {
                    repetitive_match_find(block, file, sig.ident.span().start().line, suggest)
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Whether a function is a standard conversion (`from`, `try_from`, `fmt`, …)
/// whose repetitive arms are by design, not boilerplate.
/// Operation: name membership check.
fn is_conversion_fn(sig: &syn::Signature) -> bool {
    CONVERSION_FN_NAMES.contains(&sig.ident.to_string().as_str())
}

/// The first statement in `block` that is a repetitive enum-mapping match, as a
/// BP-006 find.
/// Operation: statement scan via closures.
fn repetitive_match_find(
    block: &syn::Block,
    file: &str,
    line: usize,
    suggest: &str,
) -> Option<BoilerplateFind> {
    block.stmts.iter().find_map(|stmt| {
        let m = stmt_match(stmt)?;
        let is_mapping = !matches!(&*m.expr, syn::Expr::Tuple(_))
            && m.arms.len() >= MIN_REPETITIVE_MATCH_ARMS
            && is_repetitive_enum_mapping(&m.arms);
        is_mapping.then(|| bp006(file, line, m.arms.len(), suggest))
    })
}

/// The `match` expression a statement evaluates: a tail/expr match, or a local
/// initialized to a match. Anything else yields `None`.
/// Operation: statement match, no own calls.
fn stmt_match(stmt: &syn::Stmt) -> Option<&syn::ExprMatch> {
    match stmt {
        syn::Stmt::Expr(syn::Expr::Match(m), _) => Some(m),
        syn::Stmt::Local(local) => match local.init.as_ref().map(|i| &*i.expr) {
            Some(syn::Expr::Match(m)) => Some(m),
            _ => None,
        },
        _ => None,
    }
}

/// Build a BP-006 repetitive-match finding.
/// Operation: struct construction, no own calls.
fn bp006(file: &str, line: usize, arms: usize, suggest: &str) -> BoilerplateFind {
    BoilerplateFind {
        pattern_id: "BP-006".to_string(),
        file: file.to_string(),
        line,
        struct_name: None,
        description: format!(
            "Match with {arms} arms all mapping variants — may be replaceable with a derive"
        ),
        suggestion: suggest.to_string(),
        suppressed: false,
    }
}
