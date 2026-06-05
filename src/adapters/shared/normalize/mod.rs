//! Structural normalization of Rust function bodies into comparable token
//! streams, used by DRY fragment/duplicate detection. Split by concern: the
//! token vocabulary (`token`), operator lookup tables (`operators`), and the
//! expression (`expr` + `compound`) and pattern (`pat`) walkers. This module
//! holds the public API, the `Normalizer` state, statement-level walking, and
//! the `syn::Visit` dispatch table that routes to the per-category handlers.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use syn::visit::Visit;

mod compound;
mod expr;
mod operators;
mod pat;
mod token;

pub use token::NormalizedToken;

// ── Public API ──────────────────────────────────────────────────

/// Normalize a function body into a flat token stream.
/// Operation: creates normalizer inline (no own calls), delegates to syn visitor.
pub fn normalize_body(body: &syn::Block) -> Vec<NormalizedToken> {
    let mut n = Normalizer {
        tokens: Vec::new(),
        ident_map: HashMap::new(),
        next_ident_id: 0,
    };
    syn::visit::visit_block(&mut n, body);
    n.tokens
}

/// Normalize a slice of statements with a fresh identifier mapping.
/// Operation: creates normalizer inline, iterates statements.
/// Used for sliding-window fragment detection (Phase 5).
pub fn normalize_stmts(stmts: &[syn::Stmt]) -> Vec<NormalizedToken> {
    let mut n = Normalizer {
        tokens: Vec::new(),
        ident_map: HashMap::new(),
        next_ident_id: 0,
    };
    stmts.iter().for_each(|stmt| n.visit_stmt(stmt));
    n.tokens
}

/// Compute a structural hash from a normalized token stream.
/// Operation: hashing logic, no own calls.
pub fn structural_hash(tokens: &[NormalizedToken]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    tokens.hash(&mut hasher);
    hasher.finish()
}

/// Compute multiset Jaccard similarity between two token streams.
/// Operation: counting + arithmetic logic, no own calls.
/// Returns 1.0 for identical streams, 0.0 for completely disjoint.
pub fn jaccard_similarity(a: &[NormalizedToken], b: &[NormalizedToken]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let mut counts_a: HashMap<&NormalizedToken, usize> = HashMap::new();
    for t in a {
        *counts_a.entry(t).or_insert(0) += 1;
    }
    let mut counts_b: HashMap<&NormalizedToken, usize> = HashMap::new();
    for t in b {
        *counts_b.entry(t).or_insert(0) += 1;
    }

    let all_keys: HashSet<&NormalizedToken> =
        counts_a.keys().chain(counts_b.keys()).copied().collect();

    let mut intersection = 0usize;
    let mut union = 0usize;
    for key in all_keys {
        let ca = counts_a.get(key).copied().unwrap_or(0);
        let cb = counts_b.get(key).copied().unwrap_or(0);
        intersection += ca.min(cb);
        union += ca.max(cb);
    }

    if union == 0 {
        1.0
    } else {
        intersection as f64 / union as f64
    }
}

// ── Normalizer (private) ────────────────────────────────────────

/// AST walker that produces normalized tokens.
struct Normalizer {
    tokens: Vec<NormalizedToken>,
    ident_map: HashMap<String, usize>,
    next_ident_id: usize,
}

impl Normalizer {
    /// Resolve an identifier name to a positional index (assign on first encounter).
    fn resolve_ident(&mut self, name: &str) -> usize {
        if let Some(&id) = self.ident_map.get(name) {
            id
        } else {
            let id = self.next_ident_id;
            self.next_ident_id += 1;
            self.ident_map.insert(name.to_string(), id);
            id
        }
    }
}

// ── syn::visit::Visit implementation ────────────────────────────

impl<'ast> Visit<'ast> for Normalizer {
    fn visit_stmt(&mut self, stmt: &'ast syn::Stmt) {
        match stmt {
            syn::Stmt::Local(local) => {
                self.tokens.push(NormalizedToken::Keyword("let"));
                self.visit_pat(&local.pat);
                if let Some(init) = &local.init {
                    self.tokens.push(NormalizedToken::Operator("="));
                    self.visit_expr(&init.expr);
                    if let Some((_, diverge)) = &init.diverge {
                        self.tokens.push(NormalizedToken::Keyword("else"));
                        self.visit_expr(diverge);
                    }
                }
                self.tokens.push(NormalizedToken::Semi);
            }
            syn::Stmt::Expr(expr, semi) => {
                self.visit_expr(expr);
                if semi.is_some() {
                    self.tokens.push(NormalizedToken::Semi);
                }
            }
            syn::Stmt::Macro(m) => {
                let name = m
                    .mac
                    .path
                    .segments
                    .last()
                    .map(|s| s.ident.to_string())
                    .unwrap_or_default();
                self.tokens.push(NormalizedToken::MacroCall(name));
                self.tokens.push(NormalizedToken::Semi);
            }
            syn::Stmt::Item(_) => { /* skip items in function bodies */ }
        }
    }

    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        match expr {
            syn::Expr::Lit(lit) => self.norm_lit_kind(&lit.lit),
            syn::Expr::Path(p) => self.norm_path(p),
            syn::Expr::Binary(_) | syn::Expr::Unary(_) | syn::Expr::Assign(_) => {
                self.norm_operator(expr)
            }
            syn::Expr::Call(_) | syn::Expr::MethodCall(_) | syn::Expr::Field(_) => {
                self.norm_call_field(expr)
            }
            syn::Expr::If(_) | syn::Expr::Match(_) => self.norm_branch(expr),
            syn::Expr::ForLoop(_)
            | syn::Expr::While(_)
            | syn::Expr::Loop(_)
            | syn::Expr::Block(_) => self.norm_loop(expr),
            syn::Expr::Return(_) | syn::Expr::Break(_) | syn::Expr::Continue(_) => {
                self.norm_jump(expr)
            }
            syn::Expr::Reference(_)
            | syn::Expr::Index(_)
            | syn::Expr::Tuple(_)
            | syn::Expr::Try(_) => self.norm_compound_a(expr),
            syn::Expr::Array(_) | syn::Expr::Closure(_) | syn::Expr::Await(_) => {
                self.norm_compound_a2(expr)
            }
            syn::Expr::Range(_)
            | syn::Expr::Cast(_)
            | syn::Expr::Paren(_)
            | syn::Expr::Repeat(_) => self.norm_compound_b(expr),
            syn::Expr::Let(_)
            | syn::Expr::Struct(_)
            | syn::Expr::Yield(_)
            | syn::Expr::Macro(_) => self.norm_compound_b2(expr),
            _ => syn::visit::visit_expr(self, expr),
        }
    }

    fn visit_pat(&mut self, pat: &'ast syn::Pat) {
        match pat {
            syn::Pat::Ident(_) | syn::Pat::Wild(_) => self.norm_pat_bind(pat),
            syn::Pat::Tuple(_) | syn::Pat::TupleStruct(_) | syn::Pat::Slice(_) => {
                self.norm_pat_seq(pat)
            }
            syn::Pat::Struct(_) | syn::Pat::Reference(_) | syn::Pat::Or(_) => {
                self.norm_pat_compound(pat)
            }
            syn::Pat::Lit(_) | syn::Pat::Range(_) | syn::Pat::Rest(_) => self.norm_pat_leaf(pat),
            _ => syn::visit::visit_pat(self, pat),
        }
    }
}
