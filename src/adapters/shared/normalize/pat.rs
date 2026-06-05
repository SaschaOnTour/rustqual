//! Pattern-level normalization: per-category `syn::Pat` handlers.

use super::{NormalizedToken, Normalizer};
use syn::visit::Visit;

impl Normalizer {
    /// Binding patterns: `ident` (with optional `mut` / `@` subpattern) and `_`.
    /// Operation: per-variant emission.
    pub(super) fn norm_pat_bind(&mut self, pat: &syn::Pat) {
        match pat {
            syn::Pat::Ident(p) => {
                if p.mutability.is_some() {
                    self.tokens.push(NormalizedToken::Keyword("mut"));
                }
                let id = self.resolve_ident(&p.ident.to_string());
                self.tokens.push(NormalizedToken::Ident(id));
                if let Some((_, sub)) = &p.subpat {
                    self.tokens.push(NormalizedToken::Operator("@"));
                    self.visit_pat(sub);
                }
            }
            syn::Pat::Wild(_) => self.tokens.push(NormalizedToken::Keyword("_")),
            _ => {}
        }
    }

    /// Sequence patterns: tuples, tuple structs, slices. Operation: emission.
    pub(super) fn norm_pat_seq(&mut self, pat: &syn::Pat) {
        match pat {
            syn::Pat::Tuple(t) => {
                self.tokens.push(NormalizedToken::Keyword("tuple"));
                for elem in &t.elems {
                    self.visit_pat(elem);
                }
            }
            syn::Pat::TupleStruct(ts) => {
                self.tokens.push(NormalizedToken::Keyword("tuple"));
                for elem in &ts.elems {
                    self.visit_pat(elem);
                }
            }
            syn::Pat::Slice(s) => {
                self.tokens.push(NormalizedToken::Keyword("array"));
                for elem in &s.elems {
                    self.visit_pat(elem);
                }
            }
            _ => {}
        }
    }

    /// Struct, reference, and or-patterns. Operation: per-variant emission.
    pub(super) fn norm_pat_compound(&mut self, pat: &syn::Pat) {
        match pat {
            syn::Pat::Struct(s) => {
                self.tokens.push(NormalizedToken::Keyword("struct"));
                for field in &s.fields {
                    if let syn::Member::Named(ident) = &field.member {
                        self.tokens
                            .push(NormalizedToken::FieldAccess(ident.to_string()));
                    }
                    self.visit_pat(&field.pat);
                }
            }
            syn::Pat::Reference(r) => {
                self.tokens.push(NormalizedToken::Operator("&"));
                if r.mutability.is_some() {
                    self.tokens.push(NormalizedToken::Keyword("mut"));
                }
                self.visit_pat(&r.pat);
            }
            syn::Pat::Or(o) => {
                for (i, case) in o.cases.iter().enumerate() {
                    if i > 0 {
                        self.tokens.push(NormalizedToken::Operator("|"));
                    }
                    self.visit_pat(case);
                }
            }
            _ => {}
        }
    }

    /// Leaf patterns: literals, ranges, rest (`..`). Operation: per-variant emission.
    pub(super) fn norm_pat_leaf(&mut self, pat: &syn::Pat) {
        match pat {
            syn::Pat::Lit(l) => self.norm_lit_kind(&l.lit),
            syn::Pat::Range(r) => {
                if let Some(start) = &r.start {
                    self.visit_expr(start);
                }
                self.tokens.push(NormalizedToken::Operator(".."));
                if let Some(end) = &r.end {
                    self.visit_expr(end);
                }
            }
            syn::Pat::Rest(_) => self.tokens.push(NormalizedToken::Operator("..")),
            _ => {}
        }
    }
}
