use crate::config::StructuralConfig;
use crate::findings::Dimension;

use super::{StructuralWarning, StructuralWarningKind};

/// Detect broken trait contracts: impl Trait where methods are only stubs (TQ-like binary check).
/// Operation: iterates parsed files, inspects impl blocks, no own calls.
pub(crate) fn detect_btc(
    warnings: &mut Vec<StructuralWarning>,
    parsed: &[(String, String, syn::File)],
    config: &StructuralConfig,
) {
    if !config.check_btc {
        return;
    }
    parsed.iter().for_each(|(path, _, syntax)| {
        syntax.items.iter().for_each(|item| {
            check_item(item, path, warnings);
        });
    });
}

/// Check a single item (possibly recursing into non-test modules).
/// Operation: match dispatch + stub detection logic, own calls hidden in closures.
fn check_item(item: &syn::Item, path: &str, warnings: &mut Vec<StructuralWarning>) {
    let impl_check = |imp: &syn::ItemImpl, p: &str, w: &mut Vec<StructuralWarning>| {
        check_impl(imp, p, w);
    };
    match item {
        syn::Item::Impl(imp) => impl_check(imp, path, warnings),
        syn::Item::Mod(m) if !super::has_cfg_test_attr(&m.attrs) => {
            m.content.iter().for_each(|(_, items)| {
                items.iter().for_each(|i| check_item(i, path, warnings));
            });
        }
        _ => {}
    }
}

/// Check a single impl block for broken trait contract.
/// Operation: inspects methods for stub-only bodies.
fn check_impl(imp: &syn::ItemImpl, path: &str, warnings: &mut Vec<StructuralWarning>) {
    // Only trait impls
    let (_, trait_path, _) = match &imp.trait_ {
        Some(t) => t,
        None => return,
    };
    let trait_name = trait_path
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default();

    let methods: Vec<&syn::ImplItemFn> = imp
        .items
        .iter()
        .filter_map(|i| match i {
            syn::ImplItem::Fn(f) => Some(f),
            _ => None,
        })
        .collect();

    if methods.is_empty() {
        return;
    }

    // Flag each stub method individually
    methods
        .iter()
        .filter(|m| is_stub_body(&m.block))
        .for_each(|m| {
            let line = m.sig.ident.span().start().line;
            warnings.push(StructuralWarning {
                file: path.to_string(),
                line,
                name: m.sig.ident.to_string(),
                kind: StructuralWarningKind::BrokenTraitContract {
                    trait_name: trait_name.clone(),
                },
                dimension: Dimension::Srp,
                suppressed: false,
            });
        });
}

/// Check if a block body is a single stub expression (todo!, unimplemented!, panic!("not implemented")).
/// Operation: pattern matching on block statements and macro names.
fn is_stub_body(block: &syn::Block) -> bool {
    if block.stmts.len() != 1 {
        return false;
    }
    let expr = match &block.stmts[0] {
        syn::Stmt::Expr(expr, _) => expr,
        _ => return false,
    };
    let mac = match expr {
        syn::Expr::Macro(m) => &m.mac,
        _ => return false,
    };
    let name = mac
        .path
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default();
    match name.as_str() {
        "todo" | "unimplemented" => true,
        "panic" => {
            let tokens = mac.tokens.to_string();
            tokens.contains("not implemented") || tokens.contains("not yet implemented")
        }
        _ => false,
    }
}
