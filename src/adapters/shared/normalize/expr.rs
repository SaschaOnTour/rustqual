//! Expression-level normalization: literals, operators, calls, control flow.

use super::operators::{bin_op_str, un_op_str};
use super::{NormalizedToken, Normalizer};
use syn::visit::Visit;

impl Normalizer {
    // ── Expression category handlers ────────────────────────────
    //
    // `visit_expr` is a pure dispatch table; each handler normalizes one
    // related group of `syn::Expr` variants. Splitting by category keeps every
    // handler well under the complexity threshold while the visitor stays a flat
    // routing match.

    /// Emit the token for a literal's kind (shared by expression and pattern
    /// literals). Operation: literal-kind match, no own calls.
    pub(super) fn norm_lit_kind(&mut self, lit: &syn::Lit) {
        match lit {
            syn::Lit::Int(_) => self.tokens.push(NormalizedToken::IntLit),
            syn::Lit::Float(_) => self.tokens.push(NormalizedToken::FloatLit),
            syn::Lit::Str(_) | syn::Lit::ByteStr(_) => self.tokens.push(NormalizedToken::StrLit),
            syn::Lit::Bool(b) => self.tokens.push(NormalizedToken::BoolLit(b.value)),
            syn::Lit::Char(_) | syn::Lit::Byte(_) => self.tokens.push(NormalizedToken::CharLit),
            _ => {}
        }
    }

    /// Single-segment paths normalize to positional identifiers; multi-segment
    /// paths (external references) are dropped. Operation: ident resolution.
    pub(super) fn norm_path(&mut self, p: &syn::ExprPath) {
        if p.path.segments.len() == 1 {
            let name = p.path.segments[0].ident.to_string();
            let id = self.resolve_ident(&name);
            self.tokens.push(NormalizedToken::Ident(id));
        }
    }

    /// Binary, unary, and assignment operators. Operation: per-variant emission.
    pub(super) fn norm_operator(&mut self, expr: &syn::Expr) {
        match expr {
            syn::Expr::Binary(e) => {
                self.visit_expr(&e.left);
                self.tokens
                    .push(NormalizedToken::Operator(bin_op_str(&e.op)));
                self.visit_expr(&e.right);
            }
            syn::Expr::Unary(e) => {
                self.tokens
                    .push(NormalizedToken::Operator(un_op_str(&e.op)));
                self.visit_expr(&e.expr);
            }
            syn::Expr::Assign(e) => {
                self.visit_expr(&e.left);
                self.tokens.push(NormalizedToken::Operator("="));
                self.visit_expr(&e.right);
            }
            _ => {}
        }
    }

    /// Calls, method calls, and field access. Operation: per-variant emission.
    pub(super) fn norm_call_field(&mut self, expr: &syn::Expr) {
        match expr {
            syn::Expr::Call(e) => {
                self.visit_expr(&e.func);
                for arg in &e.args {
                    self.visit_expr(arg);
                }
            }
            syn::Expr::MethodCall(e) => {
                self.visit_expr(&e.receiver);
                self.tokens
                    .push(NormalizedToken::MethodCall(e.method.to_string()));
                for arg in &e.args {
                    self.visit_expr(arg);
                }
            }
            syn::Expr::Field(e) => {
                self.visit_expr(&e.base);
                let field_name = match &e.member {
                    syn::Member::Named(ident) => ident.to_string(),
                    syn::Member::Unnamed(idx) => idx.index.to_string(),
                };
                self.tokens.push(NormalizedToken::FieldAccess(field_name));
            }
            _ => {}
        }
    }

    /// `if` / `match` branching constructs. Operation: per-variant emission.
    pub(super) fn norm_branch(&mut self, expr: &syn::Expr) {
        match expr {
            syn::Expr::If(e) => {
                self.tokens.push(NormalizedToken::Keyword("if"));
                self.visit_expr(&e.cond);
                for stmt in &e.then_branch.stmts {
                    self.visit_stmt(stmt);
                }
                if let Some((_, else_branch)) = &e.else_branch {
                    self.tokens.push(NormalizedToken::Keyword("else"));
                    self.visit_expr(else_branch);
                }
            }
            syn::Expr::Match(e) => {
                self.tokens.push(NormalizedToken::Keyword("match"));
                self.visit_expr(&e.expr);
                for arm in &e.arms {
                    self.visit_pat(&arm.pat);
                    if let Some((_, guard)) = &arm.guard {
                        self.tokens.push(NormalizedToken::Keyword("if"));
                        self.visit_expr(guard);
                    }
                    self.tokens.push(NormalizedToken::Operator("=>"));
                    self.visit_expr(&arm.body);
                }
            }
            _ => {}
        }
    }

    /// `for` / `while` / `loop` / block loop constructs. Operation: emission.
    pub(super) fn norm_loop(&mut self, expr: &syn::Expr) {
        match expr {
            syn::Expr::ForLoop(e) => {
                self.tokens.push(NormalizedToken::Keyword("for"));
                self.visit_pat(&e.pat);
                self.tokens.push(NormalizedToken::Keyword("in"));
                self.visit_expr(&e.expr);
                for stmt in &e.body.stmts {
                    self.visit_stmt(stmt);
                }
            }
            syn::Expr::While(e) => {
                self.tokens.push(NormalizedToken::Keyword("while"));
                self.visit_expr(&e.cond);
                for stmt in &e.body.stmts {
                    self.visit_stmt(stmt);
                }
            }
            syn::Expr::Loop(e) => {
                self.tokens.push(NormalizedToken::Keyword("loop"));
                for stmt in &e.body.stmts {
                    self.visit_stmt(stmt);
                }
            }
            syn::Expr::Block(e) => {
                for stmt in &e.block.stmts {
                    self.visit_stmt(stmt);
                }
            }
            _ => {}
        }
    }

    /// `return` / `break` / `continue` jumps. Operation: per-variant emission.
    pub(super) fn norm_jump(&mut self, expr: &syn::Expr) {
        match expr {
            syn::Expr::Return(e) => {
                self.tokens.push(NormalizedToken::Keyword("return"));
                if let Some(expr) = &e.expr {
                    self.visit_expr(expr);
                }
            }
            syn::Expr::Break(e) => {
                self.tokens.push(NormalizedToken::Keyword("break"));
                if let Some(expr) = &e.expr {
                    self.visit_expr(expr);
                }
            }
            syn::Expr::Continue(_) => {
                self.tokens.push(NormalizedToken::Keyword("continue"));
            }
            _ => {}
        }
    }
}
