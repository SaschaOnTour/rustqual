use std::collections::HashSet;

use syn::visit::Visit;

use crate::config::StructuralConfig;
use crate::findings::Dimension;

use super::{StructuralWarning, StructuralWarningKind};

/// Detect self-less methods: &self/&mut self param but self never referenced in body.
/// Whole test files (in `cfg_test_files`) are skipped; inline
/// `#[cfg(test)] mod` blocks are already skipped by `visit_inherent_methods`.
/// Operation: iterates parsed files via shared visitor, no own calls.
pub(crate) fn detect_slm(
    warnings: &mut Vec<StructuralWarning>,
    parsed: &[(String, String, syn::File)],
    config: &StructuralConfig,
    cfg_test_files: &HashSet<String>,
) {
    if !config.check_slm {
        return;
    }
    super::visit_inherent_methods(parsed, |method, path| {
        if cfg_test_files.contains(path) {
            return;
        }
        check_method(method, path, warnings);
    });
}

/// Check a single method for self-less usage.
/// Operation: receiver check + body visitor, own calls hidden in closures.
fn check_method(method: &syn::ImplItemFn, path: &str, warnings: &mut Vec<StructuralWarning>) {
    let stub_check = |block: &syn::Block| is_single_stub(block);
    // Must have a receiver (self param)
    let has_receiver = method
        .sig
        .inputs
        .first()
        .map(|arg| matches!(arg, syn::FnArg::Receiver(_)))
        .unwrap_or(false);
    if !has_receiver {
        return;
    }

    // Skip empty/stub bodies (BTC handles those)
    if method.block.stmts.is_empty() || stub_check(&method.block) {
        return;
    }

    let mut checker = SelfRefChecker {
        has_self_ref: false,
    };
    checker.visit_block(&method.block);

    if !checker.has_self_ref {
        let line = method.sig.ident.span().start().line;
        warnings.push(StructuralWarning {
            file: path.to_string(),
            line,
            name: method.sig.ident.to_string(),
            kind: StructuralWarningKind::SelflessMethod,
            dimension: Dimension::Srp,
            suppressed: false,
        });
    }
}

/// Check if block is a single stub (todo!/unimplemented!/panic!).
/// Operation: pattern matching.
fn is_single_stub(block: &syn::Block) -> bool {
    if block.stmts.len() != 1 {
        return false;
    }
    match &block.stmts[0] {
        syn::Stmt::Expr(syn::Expr::Macro(m), _) => {
            let name = m
                .mac
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default();
            matches!(name.as_str(), "todo" | "unimplemented" | "panic")
        }
        _ => false,
    }
}

/// Visitor that checks if `self` is referenced anywhere in a block.
#[derive(Default)]
struct SelfRefChecker {
    has_self_ref: bool,
}

impl<'ast> Visit<'ast> for SelfRefChecker {
    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        if self.has_self_ref {
            return; // early exit
        }
        if let syn::Expr::Path(p) = expr {
            if p.path
                .segments
                .first()
                .map(|s| s.ident == "self")
                .unwrap_or(false)
            {
                self.has_self_ref = true;
                return;
            }
        }
        if let syn::Expr::Field(f) = expr {
            if matches!(&*f.base, syn::Expr::Path(p) if p.path.is_ident("self")) {
                self.has_self_ref = true;
                return;
            }
        }
        if let syn::Expr::Macro(m) = expr {
            // A macro body is an opaque token stream that syn's visitor never
            // descends into, so a `self` used only inside `format!("{}", self.x)`,
            // `matches!(self, …)`, `write!(f, "{}", self)` etc. would otherwise be
            // invisible — falsely flagging the method as self-less. Scan the raw
            // tokens for a `self` reference (robust for any macro grammar).
            if crate::adapters::shared::macro_tokens::tokens_reference_ident(&m.mac.tokens, "self")
            {
                self.has_self_ref = true;
                return;
            }
        }
        syn::visit::visit_expr(self, expr);
    }
}
