use super::{is_self_field_access, single_return_expr, BoilerplateFind};
use crate::config::sections::BoilerplateConfig;

/// Minimum getter/setter methods to flag on one struct.
const MIN_GETTER_SETTER_COUNT: usize = 3;

/// Detect structs with many trivial getter/setter methods.
/// Operation: per-file scan, per-impl detection delegated to a helper.
pub(super) fn check_manual_getter_setter(
    parsed: &[(String, String, syn::File)],
    config: &BoilerplateConfig,
) -> Vec<BoilerplateFind> {
    pattern_guard!("BP-003", config);
    parsed
        .iter()
        .flat_map(|(file, _, syntax)| {
            syntax
                .items
                .iter()
                .flat_map(|item| impl_getter_setters(item, file))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// BP-003 finds for one item: a non-trait impl carrying at least
/// `MIN_GETTER_SETTER_COUNT` trivial getter/setter methods.
/// Operation: method filter + find building via closures.
fn impl_getter_setters(item: &syn::Item, file: &str) -> Vec<BoilerplateFind> {
    let syn::Item::Impl(imp) = item else {
        return vec![];
    };
    if imp.trait_.is_some() {
        return vec![];
    }
    let methods: Vec<&syn::ImplItemFn> = imp
        .items
        .iter()
        .filter_map(|i| match i {
            syn::ImplItem::Fn(m) => Some(m),
            _ => None,
        })
        .filter(|m| is_getter_or_setter(m))
        .collect();
    if methods.len() < MIN_GETTER_SETTER_COUNT {
        return vec![];
    }
    let struct_name = impl_struct_name(imp);
    methods
        .iter()
        .map(|m| bp003(file, m.sig.ident.span().start().line, struct_name.clone()))
        .collect()
}

/// Whether `m` is a trivial getter or setter.
/// Operation: boolean combination, own calls in closures.
fn is_getter_or_setter(m: &syn::ImplItemFn) -> bool {
    is_trivial_getter(m) || is_trivial_setter(m)
}

/// One statement, `&self`, returning a `self.field` (optionally referenced).
/// Operation: shape check, own call in closure.
fn is_trivial_getter(m: &syn::ImplItemFn) -> bool {
    m.block.stmts.len() == 1
        && m.sig.inputs.len() == 1
        && single_return_expr(&m.block).is_some_and(|expr| {
            is_self_field_access(expr)
                || matches!(expr, syn::Expr::Reference(r) if is_self_field_access(&r.expr))
        })
}

/// One statement, `self` + one arg, assigning to a `self.field`.
/// Operation: shape match, own call in guard.
fn is_trivial_setter(m: &syn::ImplItemFn) -> bool {
    m.block.stmts.len() == 1
        && m.sig.inputs.len() == 2
        && matches!(
            &m.block.stmts[0],
            syn::Stmt::Expr(syn::Expr::Assign(a), Some(_)) if is_self_field_access(&a.left)
        )
}

/// The last path segment of an impl's self type, if it is a path type.
/// Operation: type match, no own calls.
fn impl_struct_name(imp: &syn::ItemImpl) -> Option<String> {
    match &*imp.self_ty {
        syn::Type::Path(tp) => tp.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

/// Build a BP-003 getter/setter finding at `file:line` on `struct_name`.
/// Operation: struct construction, no own calls.
fn bp003(file: &str, line: usize, struct_name: Option<String>) -> BoilerplateFind {
    BoilerplateFind {
        pattern_id: "BP-003".to_string(),
        file: file.to_string(),
        line,
        struct_name,
        description: "trivial getter/setter — consider field visibility or accessor macro"
            .to_string(),
        suggestion: "Consider making fields pub or using a getter/setter derive macro".to_string(),
        suppressed: false,
    }
}
