//! Expression-level normalization: compound expressions (references, ranges,
//! struct literals, closures, macros, …).

use super::{NormalizedToken, Normalizer};
use syn::visit::Visit;

impl Normalizer {
    pub(super) fn norm_compound_a(&mut self, expr: &syn::Expr) {
        match expr {
            syn::Expr::Reference(e) => {
                self.tokens.push(NormalizedToken::Operator("&"));
                if e.mutability.is_some() {
                    self.tokens.push(NormalizedToken::Keyword("mut"));
                }
                self.visit_expr(&e.expr);
            }
            syn::Expr::Index(e) => {
                self.visit_expr(&e.expr);
                self.tokens.push(NormalizedToken::Operator("[]"));
                self.visit_expr(&e.index);
            }
            syn::Expr::Tuple(e) => {
                self.tokens.push(NormalizedToken::Keyword("tuple"));
                for elem in &e.elems {
                    self.visit_expr(elem);
                }
            }
            syn::Expr::Try(e) => {
                self.visit_expr(&e.expr);
                self.tokens.push(NormalizedToken::Operator("?"));
            }
            _ => {}
        }
    }

    /// Arrays, closures, await. Operation: per-variant emission.
    pub(super) fn norm_compound_a2(&mut self, expr: &syn::Expr) {
        match expr {
            syn::Expr::Array(e) => {
                self.tokens.push(NormalizedToken::Keyword("array"));
                for elem in &e.elems {
                    self.visit_expr(elem);
                }
            }
            syn::Expr::Closure(e) => {
                self.tokens.push(NormalizedToken::Keyword("closure"));
                for input in &e.inputs {
                    self.visit_pat(input);
                }
                self.visit_expr(&e.body);
            }
            syn::Expr::Await(e) => {
                self.visit_expr(&e.base);
                self.tokens.push(NormalizedToken::Keyword("await"));
            }
            _ => {}
        }
    }

    /// Ranges, casts, parens, repeats. Operation: per-variant emission.
    pub(super) fn norm_compound_b(&mut self, expr: &syn::Expr) {
        match expr {
            syn::Expr::Range(e) => {
                if let Some(start) = &e.start {
                    self.visit_expr(start);
                }
                self.tokens.push(NormalizedToken::Operator(".."));
                if let Some(end) = &e.end {
                    self.visit_expr(end);
                }
            }
            syn::Expr::Cast(e) => {
                self.visit_expr(&e.expr);
                self.tokens.push(NormalizedToken::Keyword("as"));
            }
            syn::Expr::Paren(e) => {
                // Skip parentheses — they're structural noise
                self.visit_expr(&e.expr);
            }
            syn::Expr::Repeat(e) => {
                self.tokens.push(NormalizedToken::Keyword("array"));
                self.visit_expr(&e.expr);
                self.visit_expr(&e.len);
            }
            _ => {}
        }
    }

    /// Let-exprs, struct literals, yield, macros. Operation: per-variant emission.
    pub(super) fn norm_compound_b2(&mut self, expr: &syn::Expr) {
        match expr {
            syn::Expr::Let(e) => {
                self.tokens.push(NormalizedToken::Keyword("let"));
                self.visit_pat(&e.pat);
                self.tokens.push(NormalizedToken::Operator("="));
                self.visit_expr(&e.expr);
            }
            syn::Expr::Struct(e) => {
                self.tokens.push(NormalizedToken::Keyword("struct"));
                for field in &e.fields {
                    if let syn::Member::Named(ident) = &field.member {
                        self.tokens
                            .push(NormalizedToken::FieldAccess(ident.to_string()));
                    }
                    self.visit_expr(&field.expr);
                }
                if let Some(rest) = &e.rest {
                    self.tokens.push(NormalizedToken::Operator(".."));
                    self.visit_expr(rest);
                }
            }
            syn::Expr::Yield(e) => {
                self.tokens.push(NormalizedToken::Keyword("yield"));
                if let Some(expr) = &e.expr {
                    self.visit_expr(expr);
                }
            }
            syn::Expr::Macro(m) => {
                let name = m
                    .mac
                    .path
                    .segments
                    .last()
                    .map(|s| s.ident.to_string())
                    .unwrap_or_default();
                self.tokens.push(NormalizedToken::MacroCall(name));
            }
            _ => {}
        }
    }
}
