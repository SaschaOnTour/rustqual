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
            let id = self.aliases.index(&name);
            self.tokens.push(NormalizedToken::Ident(id));
        }
    }

    /// The callee of a call expression.
    ///
    /// A **single-segment** path keeps its name instead of becoming a
    /// positional index: `check_append(x)` and `check_rotate(x)` are different
    /// bodies, and a runner whose whole content is *which* functions it names
    /// must not match every other runner of the same length.
    ///
    /// A multi-segment path is walked as before, which drops it — see
    /// `norm_path`. Two limits in one there: `Config::default()` and
    /// `Summary::default()` share a last segment that says nothing, so naming
    /// by it would conflate more than it separates, and emitting anything at
    /// all raises the token count of every body that calls a qualified
    /// function, which changes which bodies clear `min_tokens`. Both deserve
    /// their own change; this one keeps the token count identical.
    /// Operation: shape match, own calls in the arms.
    pub(super) fn norm_callee(&mut self, func: &syn::Expr) {
        match func {
            syn::Expr::Path(p) if p.path.segments.len() == 1 => {
                let name = p.path.segments[0].ident.to_string();
                self.push_callee(name);
            }
            other => self.visit_expr(other),
        }
    }

    /// A single-segment callee: its name, unless it is a local. A callback
    /// parameter or a `let`-bound function value is a local like any other and
    /// keeps its positional index — otherwise `apply(f)` and `apply(callback)`
    /// would differ on a rename, which is exactly what alpha-renaming exists to
    /// prevent.
    /// Operation: one branch, own call in the arm.
    fn push_callee(&mut self, name: String) {
        if self.scope.is_bound(&name) {
            let id = self.aliases.index(&name);
            self.tokens.push(NormalizedToken::Ident(id));
            return;
        }
        self.tokens.push(NormalizedToken::Call(name));
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
                self.norm_callee(&e.func);
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
                // The condition may be an `if let`, whose binding covers the
                // then-branch and nothing after it — not the `else`.
                self.scoped(|n| {
                    n.tokens.push(NormalizedToken::Keyword("if"));
                    n.visit_expr(&e.cond);
                    n.visit_block(&e.then_branch);
                });
                e.else_branch.iter().for_each(|(_, else_branch)| {
                    self.tokens.push(NormalizedToken::Keyword("else"));
                    self.visit_expr(else_branch);
                });
            }
            syn::Expr::Match(e) => {
                self.tokens.push(NormalizedToken::Keyword("match"));
                self.visit_expr(&e.expr);
                // Arms are alternatives: what one binds is not in scope in the
                // next, nor after the match.
                e.arms.iter().for_each(|arm| {
                    self.scoped(|n| {
                        n.visit_pat(&arm.pat);
                        arm.guard.iter().for_each(|(_, guard)| {
                            n.tokens.push(NormalizedToken::Keyword("if"));
                            n.visit_expr(guard);
                        });
                        n.tokens.push(NormalizedToken::Operator("=>"));
                        n.visit_expr(&arm.body);
                    });
                });
            }
            _ => {}
        }
    }

    /// `for` / `while` / `loop` / block loop constructs. Operation: emission.
    pub(super) fn norm_loop(&mut self, expr: &syn::Expr) {
        match expr {
            syn::Expr::ForLoop(e) => {
                // The loop variable is not in scope in the iterator expression,
                // and it is gone after the loop.
                self.scoped(|n| {
                    n.tokens.push(NormalizedToken::Keyword("for"));
                    n.binding_after(&e.pat, |n| {
                        n.tokens.push(NormalizedToken::Keyword("in"));
                        n.visit_expr(&e.expr);
                    });
                    n.visit_block(&e.body);
                });
            }
            syn::Expr::While(e) => {
                // `while let` binds for the body and nothing after it — the
                // same reason `for` and `if` are wrapped.
                self.scoped(|n| {
                    n.tokens.push(NormalizedToken::Keyword("while"));
                    n.visit_expr(&e.cond);
                    n.visit_block(&e.body);
                });
            }
            syn::Expr::Loop(e) => {
                self.tokens.push(NormalizedToken::Keyword("loop"));
                self.visit_block(&e.body);
            }
            syn::Expr::Block(e) => {
                self.visit_block(&e.block);
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
