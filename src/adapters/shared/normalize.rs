// qual:allow(srp) reason: "Single normalizer visitor; handles many syn expression types"
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use syn::visit::Visit;

// ── Token types ─────────────────────────────────────────────────

/// A normalized AST token with variable names replaced by positional indices,
/// literal values erased to type placeholders, and structural tokens preserved.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NormalizedToken {
    /// Control flow keyword (if, for, while, match, loop, return, break, continue, let, else, etc.)
    Keyword(&'static str),
    /// Binary/unary/assignment operator as its token string.
    Operator(&'static str),
    /// Variable/parameter name replaced with first-seen positional index.
    Ident(usize),
    /// Method call — name preserved (structurally significant).
    MethodCall(String),
    /// Field access (e.g. self.field_name) — name preserved.
    FieldAccess(String),
    /// Integer literal (value erased).
    IntLit,
    /// Float literal (value erased).
    FloatLit,
    /// String/byte-string literal (value erased).
    StrLit,
    /// Boolean literal — value preserved (semantically significant).
    BoolLit(bool),
    /// Char/byte literal (value erased).
    CharLit,
    /// Macro invocation — name preserved.
    MacroCall(String),
    /// Statement terminator.
    Semi,
}

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

// ── Operator helpers ────────────────────────────────────────────

/// Convert a binary operator to its string representation.
/// Operation: pure lookup table.
fn bin_op_str(op: &syn::BinOp) -> &'static str {
    match op {
        syn::BinOp::Add(_) => "+",
        syn::BinOp::Sub(_) => "-",
        syn::BinOp::Mul(_) => "*",
        syn::BinOp::Div(_) => "/",
        syn::BinOp::Rem(_) => "%",
        syn::BinOp::And(_) => "&&",
        syn::BinOp::Or(_) => "||",
        syn::BinOp::BitXor(_) => "^",
        syn::BinOp::BitAnd(_) => "&",
        syn::BinOp::BitOr(_) => "|",
        syn::BinOp::Shl(_) => "<<",
        syn::BinOp::Shr(_) => ">>",
        syn::BinOp::Eq(_) => "==",
        syn::BinOp::Lt(_) => "<",
        syn::BinOp::Le(_) => "<=",
        syn::BinOp::Ne(_) => "!=",
        syn::BinOp::Ge(_) => ">=",
        syn::BinOp::Gt(_) => ">",
        syn::BinOp::AddAssign(_) => "+=",
        syn::BinOp::SubAssign(_) => "-=",
        syn::BinOp::MulAssign(_) => "*=",
        syn::BinOp::DivAssign(_) => "/=",
        syn::BinOp::RemAssign(_) => "%=",
        syn::BinOp::BitXorAssign(_) => "^=",
        syn::BinOp::BitAndAssign(_) => "&=",
        syn::BinOp::BitOrAssign(_) => "|=",
        syn::BinOp::ShlAssign(_) => "<<=",
        syn::BinOp::ShrAssign(_) => ">>=",
        _ => "?op",
    }
}

/// Convert a unary operator to its string representation.
/// Operation: pure lookup table.
fn un_op_str(op: &syn::UnOp) -> &'static str {
    match op {
        syn::UnOp::Deref(_) => "*",
        syn::UnOp::Not(_) => "!",
        syn::UnOp::Neg(_) => "-",
        _ => "?un",
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
            // ── Literals ────────────────────────────────────
            syn::Expr::Lit(lit) => match &lit.lit {
                syn::Lit::Int(_) => self.tokens.push(NormalizedToken::IntLit),
                syn::Lit::Float(_) => self.tokens.push(NormalizedToken::FloatLit),
                syn::Lit::Str(_) | syn::Lit::ByteStr(_) => {
                    self.tokens.push(NormalizedToken::StrLit);
                }
                syn::Lit::Bool(b) => self.tokens.push(NormalizedToken::BoolLit(b.value)),
                syn::Lit::Char(_) | syn::Lit::Byte(_) => {
                    self.tokens.push(NormalizedToken::CharLit);
                }
                _ => {}
            },

            // ── Identifiers / paths ─────────────────────────
            syn::Expr::Path(p) => {
                if p.path.segments.len() == 1 {
                    let name = p.path.segments[0].ident.to_string();
                    let id = self.resolve_ident(&name);
                    self.tokens.push(NormalizedToken::Ident(id));
                }
                // Multi-segment paths (std::io::Error, Type::method) are external
                // references — not normalized for DRY detection.
            }

            // ── Operators ───────────────────────────────────
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

            // ── Calls ───────────────────────────────────────
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

            // ── Field access ────────────────────────────────
            syn::Expr::Field(e) => {
                self.visit_expr(&e.base);
                let field_name = match &e.member {
                    syn::Member::Named(ident) => ident.to_string(),
                    syn::Member::Unnamed(idx) => idx.index.to_string(),
                };
                self.tokens.push(NormalizedToken::FieldAccess(field_name));
            }

            // ── Control flow ────────────────────────────────
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

            // ── Jump statements ─────────────────────────────
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

            // ── Compound expressions ────────────────────────
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
            syn::Expr::Try(e) => {
                self.visit_expr(&e.expr);
                self.tokens.push(NormalizedToken::Operator("?"));
            }
            syn::Expr::Await(e) => {
                self.visit_expr(&e.base);
                self.tokens.push(NormalizedToken::Keyword("await"));
            }
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

            // ── Fallback: let syn's default visitor recurse ─
            _ => {
                syn::visit::visit_expr(self, expr);
            }
        }
    }

    fn visit_pat(&mut self, pat: &'ast syn::Pat) {
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
            syn::Pat::Wild(_) => {
                self.tokens.push(NormalizedToken::Keyword("_"));
            }
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
            syn::Pat::Lit(l) => {
                // PatLit is ExprLit — handle the literal directly
                match &l.lit {
                    syn::Lit::Int(_) => self.tokens.push(NormalizedToken::IntLit),
                    syn::Lit::Float(_) => self.tokens.push(NormalizedToken::FloatLit),
                    syn::Lit::Str(_) | syn::Lit::ByteStr(_) => {
                        self.tokens.push(NormalizedToken::StrLit);
                    }
                    syn::Lit::Bool(b) => {
                        self.tokens.push(NormalizedToken::BoolLit(b.value));
                    }
                    syn::Lit::Char(_) | syn::Lit::Byte(_) => {
                        self.tokens.push(NormalizedToken::CharLit);
                    }
                    _ => {}
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
            syn::Pat::Slice(s) => {
                self.tokens.push(NormalizedToken::Keyword("array"));
                for elem in &s.elems {
                    self.visit_pat(elem);
                }
            }
            syn::Pat::Rest(_) => {
                self.tokens.push(NormalizedToken::Operator(".."));
            }
            syn::Pat::Range(r) => {
                if let Some(start) = &r.start {
                    self.visit_expr(start);
                }
                self.tokens.push(NormalizedToken::Operator(".."));
                if let Some(end) = &r.end {
                    self.visit_expr(end);
                }
            }
            _ => {
                syn::visit::visit_pat(self, pat);
            }
        }
    }
}
