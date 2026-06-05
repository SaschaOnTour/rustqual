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

    // ── Expression category handlers ────────────────────────────
    //
    // `visit_expr` is a pure dispatch table; each handler normalizes one
    // related group of `syn::Expr` variants. Splitting by category keeps every
    // handler well under the complexity threshold while the visitor stays a flat
    // routing match.

    /// Emit the token for a literal's kind (shared by expression and pattern
    /// literals). Operation: literal-kind match, no own calls.
    fn norm_lit_kind(&mut self, lit: &syn::Lit) {
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
    fn norm_path(&mut self, p: &syn::ExprPath) {
        if p.path.segments.len() == 1 {
            let name = p.path.segments[0].ident.to_string();
            let id = self.resolve_ident(&name);
            self.tokens.push(NormalizedToken::Ident(id));
        }
    }

    /// Binary, unary, and assignment operators. Operation: per-variant emission.
    fn norm_operator(&mut self, expr: &syn::Expr) {
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
    fn norm_call_field(&mut self, expr: &syn::Expr) {
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
    fn norm_branch(&mut self, expr: &syn::Expr) {
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
    fn norm_loop(&mut self, expr: &syn::Expr) {
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
    fn norm_jump(&mut self, expr: &syn::Expr) {
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

    /// References, indexing, tuples, try. Operation: per-variant emission.
    fn norm_compound_a(&mut self, expr: &syn::Expr) {
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
    fn norm_compound_a2(&mut self, expr: &syn::Expr) {
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
    fn norm_compound_b(&mut self, expr: &syn::Expr) {
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
    fn norm_compound_b2(&mut self, expr: &syn::Expr) {
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

    // ── Pattern category handlers ───────────────────────────────

    /// Binding patterns: `ident` (with optional `mut` / `@` subpattern) and `_`.
    /// Operation: per-variant emission.
    fn norm_pat_bind(&mut self, pat: &syn::Pat) {
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
    fn norm_pat_seq(&mut self, pat: &syn::Pat) {
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
    fn norm_pat_compound(&mut self, pat: &syn::Pat) {
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
    fn norm_pat_leaf(&mut self, pat: &syn::Pat) {
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
