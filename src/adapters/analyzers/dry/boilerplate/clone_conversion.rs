use super::{count_field_clones, BoilerplateFind};
use crate::config::sections::BoilerplateConfig;

/// Minimum cloned fields for clone-heavy conversion detection.
const MIN_CLONE_FIELDS: usize = 3;

/// Detect struct construction with many `.clone()` calls on fields.
/// Operation: per-file scan, detection delegated to helpers in closures.
pub(super) fn check_clone_heavy_conversion(
    parsed: &[(String, String, syn::File)],
    config: &BoilerplateConfig,
) -> Vec<BoilerplateFind> {
    pattern_guard!("BP-008", config);
    parsed
        .iter()
        .flat_map(|(file, _, syntax)| {
            syntax
                .items
                .iter()
                .flat_map(item_fn_bodies)
                .filter_map(|(block, line)| clone_heavy_find(block, file, line))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Yield each `(body, decl-line)` for an item: a free fn, or every method of an
/// impl block. Other items contribute nothing.
/// Operation: item match + impl-method filter in a closure.
fn item_fn_bodies(item: &syn::Item) -> Vec<(&syn::Block, usize)> {
    match item {
        syn::Item::Fn(f) => vec![(&f.block, f.sig.ident.span().start().line)],
        syn::Item::Impl(imp) => imp
            .items
            .iter()
            .filter_map(|sub| match sub {
                syn::ImplItem::Fn(m) => Some((&m.block, m.sig.ident.span().start().line)),
                _ => None,
            })
            .collect(),
        _ => vec![],
    }
}

/// Find the first statement in `block` whose value is a struct construction with
/// `>= MIN_CLONE_FIELDS` cloned fields, as a BP-008 find.
/// Operation: statement scan via closures.
fn clone_heavy_find(block: &syn::Block, file: &str, line: usize) -> Option<BoilerplateFind> {
    block.stmts.iter().find_map(|stmt| {
        let clones = count_field_clones(stmt_value(stmt)?);
        (clones >= MIN_CLONE_FIELDS).then(|| bp008(file, line, clones))
    })
}

/// The expression a statement evaluates: a tail/expr statement, or a local's
/// initializer. Items and empty locals have no value.
/// Operation: statement match, no own calls.
fn stmt_value(stmt: &syn::Stmt) -> Option<&syn::Expr> {
    match stmt {
        syn::Stmt::Expr(e, _) => Some(e),
        syn::Stmt::Local(local) => local.init.as_ref().map(|init| &*init.expr),
        _ => None,
    }
}

/// Build the BP-008 finding for `clones` cloned fields at `file:line`.
/// Operation: struct construction, no own calls.
fn bp008(file: &str, line: usize, clones: usize) -> BoilerplateFind {
    BoilerplateFind {
        pattern_id: "BP-008".to_string(),
        file: file.to_string(),
        line,
        struct_name: None,
        description: format!(
            "Struct construction with {clones} .clone() calls — consider Into/From or ownership transfer"
        ),
        suggestion: "Consider implementing From/Into or restructuring to avoid cloning".to_string(),
        suppressed: false,
    }
}
