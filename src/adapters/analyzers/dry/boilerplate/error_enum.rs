use syn::spanned::Spanned;

use super::{self_type_of, trait_name_of, BoilerplateFind};
use crate::config::sections::BoilerplateConfig;

/// Minimum From impls to flag as error enum boilerplate.
const MIN_FROM_IMPLS_FOR_ERROR: usize = 3;

/// Detect enums with multiple trivial `impl From<T>` for wrapping errors.
/// Operation: per-file grouping, detection delegated to helpers in closures.
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
    parsed
        .iter()
        .flat_map(|(file, _, syntax)| {
            count_from_impls(syntax)
                .into_iter()
                .filter(|(_, (count, _))| *count >= MIN_FROM_IMPLS_FOR_ERROR)
                .map(|(name, (count, line))| bp007(file, line, &name, count, suggest))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Group trivial `From<_>` impls by their target type → `(count, first line)`.
/// Operation: filter_map + fold via closures.
fn count_from_impls(syntax: &syn::File) -> std::collections::HashMap<String, (usize, usize)> {
    let mut counts: std::collections::HashMap<String, (usize, usize)> =
        std::collections::HashMap::new();
    syntax
        .items
        .iter()
        .filter_map(trivial_from_impl)
        .for_each(|(name, line)| {
            counts.entry(name).or_insert((0, line)).0 += 1;
        });
    counts
}

/// `(target type, decl line)` for an item that is a single-method `from`
/// `impl From<_>` with a one-expression body, else `None`.
/// Operation: trait/type checks via helpers in closures.
fn trivial_from_impl(item: &syn::Item) -> Option<(String, usize)> {
    let syn::Item::Impl(imp) = item else {
        return None;
    };
    if trait_name_of(imp).as_deref() != Some("From") || !is_single_expr_from(imp) {
        return None;
    }
    Some((self_type_of(imp)?, imp.self_ty.span().start().line))
}

/// Whether an impl has exactly one `from` method with a single-expression body.
/// Operation: method filter + shape match in closures.
fn is_single_expr_from(imp: &syn::ItemImpl) -> bool {
    let methods: Vec<_> = imp
        .items
        .iter()
        .filter_map(|i| match i {
            syn::ImplItem::Fn(m) => Some(m),
            _ => None,
        })
        .collect();
    methods.len() == 1
        && methods[0].sig.ident == "from"
        && methods[0].block.stmts.len() == 1
        && matches!(&methods[0].block.stmts[0], syn::Stmt::Expr(_, None))
}

/// Build a BP-007 error-enum finding.
/// Operation: struct construction, no own calls.
fn bp007(file: &str, line: usize, type_name: &str, count: usize, suggest: &str) -> BoilerplateFind {
    BoilerplateFind {
        pattern_id: "BP-007".to_string(),
        file: file.to_string(),
        line,
        struct_name: Some(type_name.to_string()),
        description: format!(
            "{count} trivial From impls for {type_name} — error enum boilerplate"
        ),
        suggestion: suggest.to_string(),
        suppressed: false,
    }
}
