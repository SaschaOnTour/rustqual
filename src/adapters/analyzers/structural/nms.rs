use std::collections::HashSet;

use syn::visit::Visit;

use crate::config::StructuralConfig;
use crate::findings::Dimension;

use super::{StructuralWarning, StructuralWarningKind};

/// Detect needless &mut self: method takes &mut self but never writes to self.
/// Whole test files (in `cfg_test_files`) are skipped; inline
/// `#[cfg(test)] mod` blocks are already skipped by `visit_inherent_methods`.
/// Operation: iterates parsed files via shared visitor, no own calls.
pub(crate) fn detect_nms(
    warnings: &mut Vec<StructuralWarning>,
    parsed: &[(String, String, syn::File)],
    config: &StructuralConfig,
    cfg_test_files: &HashSet<String>,
) {
    if !config.check_nms {
        return;
    }
    super::visit_inherent_methods(parsed, |method, path| {
        if cfg_test_files.contains(path) {
            return;
        }
        check_method(method, path, warnings);
    });
}

/// Check a single method for needless &mut self.
/// Operation: receiver mutability check + mutation visitor.
fn check_method(method: &syn::ImplItemFn, path: &str, warnings: &mut Vec<StructuralWarning>) {
    let is_mut_self = method
        .sig
        .inputs
        .first()
        .and_then(|arg| match arg {
            syn::FnArg::Receiver(r) => Some(r.mutability.is_some() && r.reference.is_some()),
            _ => None,
        })
        .unwrap_or(false);
    if !is_mut_self {
        return;
    }

    // Skip empty/stub bodies
    if method.block.stmts.is_empty() {
        return;
    }

    let mut checker = MutationChecker {
        has_mutation: false,
        has_self_ref: false,
    };
    checker.visit_block(&method.block);

    // Only flag if self IS referenced (otherwise SLM catches it) but never mutated
    if checker.has_self_ref && !checker.has_mutation {
        let line = method.sig.ident.span().start().line;
        warnings.push(StructuralWarning {
            file: path.to_string(),
            line,
            name: method.sig.ident.to_string(),
            kind: StructuralWarningKind::NeedlessMutSelf,
            dimension: Dimension::Srp,
            suppressed: false,
        });
    }
}

/// Visitor that checks if `self` is mutated anywhere in a block.
/// Conservative: any method call on self is assumed to potentially mutate.
#[derive(Default)]
struct MutationChecker {
    has_mutation: bool,
    has_self_ref: bool,
}

impl<'ast> Visit<'ast> for MutationChecker {
    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        self.has_self_ref |= is_self_ref(expr);
        self.has_mutation |= is_self_mutation(expr);
        if !self.has_mutation {
            syn::visit::visit_expr(self, expr);
        }
    }
}

/// Whether `expr` mutates `self`: `self.field = …`, `self.field[i] = …`,
/// compound assignment to `self.field`, a method call on `self.field`, or
/// `&mut self.field`. Conservative — any method call on a self field counts.
/// Operation: pattern matching against self-target shapes, no own calls counted.
fn is_self_mutation(expr: &syn::Expr) -> bool {
    match expr {
        syn::Expr::Assign(a) => is_self_target(&a.left),
        // Compound assignments: +=, -=, *=, etc.
        syn::Expr::Binary(b) => is_compound_assign(&b.op) && is_self_target(&b.left),
        // Any method call on self or on part of self is conservatively a mutation
        syn::Expr::MethodCall(mc) => is_self_path(&mc.receiver) || is_self_target(&mc.receiver),
        syn::Expr::Reference(r) => r.mutability.is_some() && is_self_target(&r.expr),
        _ => false,
    }
}

/// Whether the expression designates part of `self`: `self.field`,
/// `self.a.b`, `self.field[i]`, `(self.field)`, `(*self).field`. The chain is
/// followed to its
/// root, because a nested field is just as much part of `self` as a direct one
/// — `self.inner.items.push(v)` mutates `self`, and stopping at one level
/// reports a needless `&mut` on a method that plainly needs it.
/// Operation: chain walk to the root, no own calls.
fn is_self_target(expr: &syn::Expr) -> bool {
    let mut current = expr;
    loop {
        current = match current {
            syn::Expr::Field(f) => &f.base,
            syn::Expr::Index(idx) => &idx.expr,
            syn::Expr::Paren(p) => &p.expr,
            syn::Expr::Group(g) => &g.expr,
            syn::Expr::Unary(u) if matches!(u.op, syn::UnOp::Deref(_)) => &u.expr,
            syn::Expr::Path(p) => return p.path.is_ident("self"),
            _ => return false,
        };
    }
}

/// Check if a binary operator is a compound assignment (+=, -=, *=, etc.).
/// Operation: pattern matching.
fn is_compound_assign(op: &syn::BinOp) -> bool {
    matches!(
        op,
        syn::BinOp::AddAssign(_)
            | syn::BinOp::SubAssign(_)
            | syn::BinOp::MulAssign(_)
            | syn::BinOp::DivAssign(_)
            | syn::BinOp::RemAssign(_)
            | syn::BinOp::BitAndAssign(_)
            | syn::BinOp::BitOrAssign(_)
            | syn::BinOp::BitXorAssign(_)
            | syn::BinOp::ShlAssign(_)
            | syn::BinOp::ShrAssign(_)
    )
}

/// Check if expression is `self`.
/// Operation: pattern matching.
fn is_self_path(expr: &syn::Expr) -> bool {
    matches!(expr, syn::Expr::Path(p) if p.path.is_ident("self"))
}

/// Check if expression references self in any way.
/// Operation: pattern matching.
fn is_self_ref(expr: &syn::Expr) -> bool {
    match expr {
        syn::Expr::Path(p) => p
            .path
            .segments
            .first()
            .map(|s| s.ident == "self")
            .unwrap_or(false),
        syn::Expr::Field(f) => matches!(&*f.base, syn::Expr::Path(p) if p.path.is_ident("self")),
        _ => false,
    }
}
