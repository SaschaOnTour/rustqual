use syn::visit::Visit;

use crate::config::StructuralConfig;
use crate::findings::Dimension;

use super::{StructuralWarning, StructuralWarningKind};

/// Detect needless &mut self: method takes &mut self but never writes to self.
/// Operation: iterates parsed files via shared visitor, no own calls.
pub(crate) fn detect_nms(
    warnings: &mut Vec<StructuralWarning>,
    parsed: &[(String, String, syn::File)],
    config: &StructuralConfig,
) {
    if !config.check_nms {
        return;
    }
    super::visit_inherent_methods(parsed, |method, path| {
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
        // Track self references
        if is_self_ref(expr) {
            self.has_self_ref = true;
        }
        // Check for mutations: self.field = ..., self.field[i] = ...,
        // self.field -= ..., self.field.method(), &mut self.field
        match expr {
            syn::Expr::Assign(a) if is_self_target(&a.left) => {
                self.has_mutation = true;
            }
            // Compound assignments: +=, -=, *=, etc.
            syn::Expr::Binary(b) if is_compound_assign(&b.op) && is_self_target(&b.left) => {
                self.has_mutation = true;
            }
            // Any method call on self.field or self.field[i] is conservatively a mutation
            syn::Expr::MethodCall(mc)
                if is_self_field(&mc.receiver)
                    || is_self_path(&mc.receiver)
                    || is_self_indexed_field(&mc.receiver) =>
            {
                self.has_mutation = true;
            }
            syn::Expr::Reference(r) if r.mutability.is_some() && is_self_target(&r.expr) => {
                self.has_mutation = true;
            }
            _ => {}
        }
        if !self.has_mutation {
            syn::visit::visit_expr(self, expr);
        }
    }
}

/// Check if expression is a mutation target involving self: `self.field`, `self.field[i]`.
/// Operation: pattern matching, no own calls.
fn is_self_target(expr: &syn::Expr) -> bool {
    match expr {
        syn::Expr::Field(f) => matches!(&*f.base, syn::Expr::Path(p) if p.path.is_ident("self")),
        syn::Expr::Index(idx) => {
            matches!(&*idx.expr, syn::Expr::Field(f) if matches!(&*f.base, syn::Expr::Path(p) if p.path.is_ident("self")))
        }
        _ => false,
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

/// Check if expression is `self.field`.
/// Operation: pattern matching.
fn is_self_field(expr: &syn::Expr) -> bool {
    match expr {
        syn::Expr::Field(f) => matches!(&*f.base, syn::Expr::Path(p) if p.path.is_ident("self")),
        _ => false,
    }
}

/// Check if expression is `self.field[i]` (indexed field access).
/// Operation: pattern matching.
fn is_self_indexed_field(expr: &syn::Expr) -> bool {
    matches!(expr, syn::Expr::Index(idx) if is_self_field(&idx.expr))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn detect_in(source: &str) -> Vec<StructuralWarning> {
        let syntax = syn::parse_file(source).expect("test source");
        let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
        let config = StructuralConfig::default();
        let mut warnings = Vec::new();
        detect_nms(&mut warnings, &parsed, &config);
        warnings
    }

    #[test]
    fn test_needless_mut_self_flagged() {
        let w = detect_in("struct S { x: i32 } impl S { fn foo(&mut self) -> i32 { self.x } }");
        assert_eq!(w.len(), 1);
        assert!(matches!(w[0].kind, StructuralWarningKind::NeedlessMutSelf));
    }

    #[test]
    fn test_assignment_not_flagged() {
        let w =
            detect_in("struct S { x: i32 } impl S { fn set(&mut self, v: i32) { self.x = v; } }");
        assert!(w.is_empty());
    }

    #[test]
    fn test_method_call_on_self_not_flagged() {
        let w = detect_in("struct S { items: Vec<i32> } impl S { fn add(&mut self, v: i32) { self.items.push(v); } }");
        assert!(w.is_empty());
    }

    #[test]
    fn test_mut_borrow_not_flagged() {
        let w = detect_in(
            "struct S { x: i32 } impl S { fn borrow(&mut self) -> &mut i32 { &mut self.x } }",
        );
        assert!(w.is_empty());
    }

    #[test]
    fn test_immutable_self_not_checked() {
        let w = detect_in("struct S { x: i32 } impl S { fn foo(&self) -> i32 { self.x } }");
        assert!(w.is_empty());
    }

    #[test]
    fn test_trait_impl_excluded() {
        let w = detect_in("trait T { fn foo(&mut self); } struct S { x: i32 } impl T for S { fn foo(&mut self) { let _ = self.x; } }");
        assert!(w.is_empty());
    }

    #[test]
    fn test_no_self_ref_skipped_for_slm() {
        // If self is never referenced, SLM catches it — NMS should not fire
        let w = detect_in("struct S; impl S { fn foo(&mut self) -> i32 { 42 } }");
        assert!(w.is_empty());
    }

    #[test]
    fn test_empty_body_not_flagged() {
        let w = detect_in("struct S; impl S { fn foo(&mut self) {} }");
        assert!(w.is_empty());
    }

    #[test]
    fn test_indexed_field_method_call_not_flagged() {
        let w = detect_in(
            "struct S { items: Vec<Vec<i32>> } impl S { fn add(&mut self, i: usize, v: i32) { self.items[i].push(v); } }",
        );
        assert!(
            w.is_empty(),
            "self.items[i].push(v) should be recognized as mutation"
        );
    }
}
