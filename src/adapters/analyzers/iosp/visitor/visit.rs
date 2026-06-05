//! The `BodyVisitor` expression walk.
//!
//! `visit_expr` is a pure dispatch table; the per-category walk steps are
//! free functions taking `&mut BodyVisitor` rather than methods. They are
//! decomposition of one walk, not separate responsibilities of the type, so
//! keeping them out of the inherent-method surface keeps the struct's SRP
//! footprint honest (a metrics-collecting visitor already carries many fields).

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, ExprCall, ExprMethodCall};

use super::{is_delegation_only_body, is_match_dispatch, is_trivial_match_arm, BodyVisitor};
use crate::adapters::analyzers::iosp::types::CallOccurrence;

impl<'a, 'ast> Visit<'ast> for BodyVisitor<'a> {
    fn visit_expr(&mut self, expr: &'ast Expr) {
        // Reset boolean alternation tracking for non-boolean expressions.
        if !matches!(expr, Expr::Binary(b) if matches!(b.op, syn::BinOp::And(_) | syn::BinOp::Or(_)))
        {
            self.last_boolean_op = None;
        }

        // Pure dispatch: each walk step normalizes one related group of variants.
        match expr {
            Expr::If(_) | Expr::Match(_) => walk_branch(self, expr),
            Expr::ForLoop(_) | Expr::While(_) | Expr::Loop(_) => walk_loop(self, expr),
            Expr::Try(_) => walk_try(self, expr),
            Expr::Binary(_) => walk_binary(self, expr),
            Expr::Unsafe(_) | Expr::Closure(_) | Expr::Async(_) | Expr::Await(_) => {
                walk_scoped(self, expr)
            }
            Expr::Index(_) => walk_index(self, expr),
            Expr::Lit(_) | Expr::Unary(_) => walk_literal(self, expr),
            Expr::Macro(_) => walk_macro(self, expr),
            Expr::Call(_) => walk_call(self, expr),
            Expr::MethodCall(_) => walk_method_call(self, expr),
            _ => syn::visit::visit_expr(self, expr),
        }
    }

    /// Track const item context to suppress magic number detection.
    fn visit_item_const(&mut self, i: &'ast syn::ItemConst) {
        self.in_const_context += 1;
        syn::visit::visit_item_const(self, i);
        self.in_const_context -= 1;
    }

    /// Track static item context to suppress magic number detection.
    fn visit_item_static(&mut self, i: &'ast syn::ItemStatic) {
        self.in_const_context += 1;
        syn::visit::visit_item_static(self, i);
        self.in_const_context -= 1;
    }
}

/// `if` / `match` branching: complexity + nesting + logic, then recurse.
/// Operation: per-variant metric updates.
fn walk_branch(v: &mut BodyVisitor<'_>, expr: &Expr) {
    match expr {
        Expr::If(expr_if) => {
            // Complexity: always tracked (regardless of closure leniency)
            v.cognitive_complexity += 1 + v.nesting_depth;
            v.cyclomatic_complexity += 1;
            v.record_hotspot("if", expr_if.if_token.span);
            // IOSP: respects closure leniency
            v.record_logic("if", expr_if.if_token.span);
            v.enter_nesting();
            syn::visit::visit_expr(v, expr);
            v.exit_nesting();
        }
        Expr::Match(expr_match) => {
            v.cognitive_complexity += 1 + v.nesting_depth;
            // Cyclomatic: only non-trivial arms count (trivial lookup arms excluded)
            let non_trivial = expr_match
                .arms
                .iter()
                .filter(|arm| !is_trivial_match_arm(arm))
                .count();
            v.cyclomatic_complexity += non_trivial.saturating_sub(1);
            v.record_hotspot("match", expr_match.match_token.span);
            if !is_match_dispatch(&expr_match.arms) {
                v.record_logic("match", expr_match.match_token.span);
            }
            v.enter_nesting();
            syn::visit::visit_expr(v, expr);
            v.exit_nesting();
        }
        _ => {}
    }
}

/// `for` / `while` / `loop`: complexity + nesting + logic, then recurse.
/// Operation: per-variant metric updates.
fn walk_loop(v: &mut BodyVisitor<'_>, expr: &Expr) {
    match expr {
        Expr::ForLoop(expr_for) => {
            v.cognitive_complexity += 1 + v.nesting_depth;
            v.cyclomatic_complexity += 1;
            v.record_hotspot("for", expr_for.for_token.span);
            if !is_delegation_only_body(&expr_for.body.stmts) {
                v.record_logic("for", expr_for.for_token.span);
            }
            // Skip logic detection in the iterator expression (range, .len(), etc.)
            v.in_for_iter = true;
            v.visit_expr(&expr_for.expr);
            v.in_for_iter = false;
            v.visit_pat(&expr_for.pat);
            v.enter_nesting();
            v.visit_block(&expr_for.body);
            v.exit_nesting();
        }
        Expr::While(expr_while) => {
            v.cognitive_complexity += 1 + v.nesting_depth;
            v.cyclomatic_complexity += 1;
            v.record_hotspot("while", expr_while.while_token.span);
            v.record_logic("while", expr_while.while_token.span);
            v.enter_nesting();
            syn::visit::visit_expr(v, expr);
            v.exit_nesting();
        }
        Expr::Loop(expr_loop) => {
            v.cognitive_complexity += 1 + v.nesting_depth;
            v.cyclomatic_complexity += 1;
            v.record_hotspot("loop", expr_loop.loop_token.span);
            v.record_logic("loop", expr_loop.loop_token.span);
            v.enter_nesting();
            syn::visit::visit_expr(v, expr);
            v.exit_nesting();
        }
        _ => {}
    }
}

/// `?` operator: logic only under `strict_error_propagation`, then recurse.
/// Operation: config gate + recurse.
fn walk_try(v: &mut BodyVisitor<'_>, expr: &Expr) {
    if let Expr::Try(expr_try) = expr {
        if v.config.strict_error_propagation {
            v.record_logic("?", expr_try.question_token.span());
        }
    }
    syn::visit::visit_expr(v, expr);
}

/// Binary operators: record arithmetic/boolean/comparison/bitwise as logic,
/// track boolean-operator complexity, then recurse.
/// Operation: logic-kind classification + alternation tracking.
fn walk_binary(v: &mut BodyVisitor<'_>, expr: &Expr) {
    if let Expr::Binary(expr_bin) = expr {
        if let Some(kind) = binary_logic_kind(&expr_bin.op) {
            v.record_logic(kind, expr_bin.op.span());
        }
        track_boolean_op(v, &expr_bin.op);
    }
    syn::visit::visit_expr(v, expr);
}

/// `&&` / `||`: add to cyclomatic complexity and charge a cognitive cost when
/// the boolean operator alternates from the previous one. Operation: arithmetic.
fn track_boolean_op(v: &mut BodyVisitor<'_>, op: &syn::BinOp) {
    let is_and = match op {
        syn::BinOp::And(_) => true,
        syn::BinOp::Or(_) => false,
        _ => return,
    };
    v.cyclomatic_complexity += 1;
    if let Some(last) = v.last_boolean_op {
        if last != is_and {
            v.cognitive_complexity += 1;
        }
    }
    v.last_boolean_op = Some(is_and);
}

/// `unsafe` / closures / async blocks / `.await`: depth tracking, then recurse.
/// Operation: per-variant depth bookkeeping.
fn walk_scoped(v: &mut BodyVisitor<'_>, expr: &Expr) {
    match expr {
        Expr::Unsafe(_) => {
            v.unsafe_block_count += 1;
            syn::visit::visit_expr(v, expr);
        }
        Expr::Closure(_) => {
            v.closure_depth += 1;
            syn::visit::visit_expr(v, expr);
            v.closure_depth -= 1;
        }
        Expr::Async(_) => {
            v.async_block_depth += 1;
            syn::visit::visit_expr(v, expr);
            v.async_block_depth -= 1;
        }
        // `.await` is not logic — recurse without bookkeeping.
        _ => syn::visit::visit_expr(v, expr),
    }
}

/// Array index: suppress magic-number detection inside the index expression.
/// Operation: context-flagged recursion (no default recurse).
fn walk_index(v: &mut BodyVisitor<'_>, expr: &Expr) {
    if let Expr::Index(expr_index) = expr {
        v.visit_expr(&expr_index.expr);
        v.in_index_context += 1;
        v.visit_expr(&expr_index.index);
        v.in_index_context -= 1;
    }
}

/// Numeric literals (and negated numeric literals): magic-number detection.
/// A `-1` is recorded as a unit to avoid double-counting the inner literal.
/// Operation: literal-shape match + record.
fn walk_literal(v: &mut BodyVisitor<'_>, expr: &Expr) {
    match expr {
        Expr::Lit(expr_lit) => {
            record_numeric_lit(v, &expr_lit.lit, "");
            syn::visit::visit_expr(v, expr);
        }
        Expr::Unary(expr_unary) => {
            if matches!(expr_unary.op, syn::UnOp::Neg(_)) {
                if let Expr::Lit(expr_lit) = &*expr_unary.expr {
                    if matches!(expr_lit.lit, syn::Lit::Int(_) | syn::Lit::Float(_)) {
                        record_numeric_lit(v, &expr_lit.lit, "-");
                        // Skip default recursion — inner literal already handled.
                        return;
                    }
                }
            }
            syn::visit::visit_expr(v, expr);
        }
        _ => {}
    }
}

/// Record an int/float literal as a magic number, with an optional sign prefix.
/// Operation: literal-kind match + record.
fn record_numeric_lit(v: &mut BodyVisitor<'_>, lit: &syn::Lit, prefix: &str) {
    match lit {
        syn::Lit::Int(lit_int) => {
            v.record_magic_number(
                format!("{prefix}{}", lit_int.base10_digits()),
                lit_int.span(),
            );
        }
        syn::Lit::Float(lit_float) => {
            v.record_magic_number(
                format!("{prefix}{}", lit_float.base10_digits()),
                lit_float.span(),
            );
        }
        _ => {}
    }
}

/// `panic!` / `unreachable!` / `todo!` macros: error-handling counts, then recurse.
/// Operation: macro-name match + count.
fn walk_macro(v: &mut BodyVisitor<'_>, expr: &Expr) {
    if let Expr::Macro(expr_macro) = expr {
        let macro_name = expr_macro
            .mac
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string());
        match macro_name.as_deref() {
            Some("panic") | Some("unreachable") => v.panic_count += 1,
            Some("todo") => v.todo_count += 1,
            _ => {}
        }
    }
    syn::visit::visit_expr(v, expr);
}

/// Free-function calls: record own-function calls (honouring leniency and
/// `allow_recursion`), then recurse. Operation: name resolution + own-call push.
fn walk_call(v: &mut BodyVisitor<'_>, expr: &Expr) {
    // The predicate own-calls live in a closure so this stays an Operation
    // (closure-internal calls are lenient), not an IOSP Violation.
    let resolve = |s: &BodyVisitor<'_>, func: &Expr| -> Option<CallOccurrence> {
        if s.in_lenient_nested_context() {
            return None;
        }
        let name = BodyVisitor::extract_call_name(func)?;
        let recursive = s.config.allow_recursion && s.is_recursive_call(&name);
        (!recursive && s.scope.is_own_function(&name)).then(|| CallOccurrence {
            name,
            line: func.span().start().line,
        })
    };
    if let Expr::Call(ExprCall { func, .. }) = expr {
        if let Some(occ) = resolve(v, func) {
            v.own_calls.push(occ);
        }
    }
    syn::visit::visit_expr(v, expr);
}

/// Method calls: error-handling counts (always) + own-method calls (honouring
/// leniency, iterator adaptors, and `allow_recursion`), then recurse.
/// Operation: method-name classification + own-call push.
fn walk_method_call(v: &mut BodyVisitor<'_>, expr: &Expr) {
    let Expr::MethodCall(ExprMethodCall {
        method, receiver, ..
    }) = expr
    else {
        return;
    };
    let method_name = method.to_string();
    // Error handling: count in ALL contexts (including closures)
    match method_name.as_str() {
        "unwrap" => v.unwrap_count += 1,
        "expect" => v.expect_count += 1,
        _ => {}
    }
    // Predicate own-calls hidden in a closure (lenient) — keeps this an Operation.
    let resolve = |s: &BodyVisitor<'_>| -> Option<CallOccurrence> {
        if s.in_lenient_nested_context() {
            return None;
        }
        let is_iterator = BodyVisitor::is_iterator_method(&method_name);
        let recursive = s.config.allow_recursion && s.is_recursive_call(&method_name);
        let skip = (is_iterator && !s.config.strict_iterator_chains) || recursive;
        (!skip && s.is_type_resolved_own_method(&method_name, receiver)).then(|| CallOccurrence {
            name: format!(".{method_name}()"),
            line: method.span().start().line,
        })
    };
    if let Some(occ) = resolve(v) {
        v.own_calls.push(occ);
    }
    syn::visit::visit_expr(v, expr);
}

/// Classify a binary operator as the IOSP "logic kind" it represents, if any.
/// Operation: pure operator-group lookup.
fn binary_logic_kind(op: &syn::BinOp) -> Option<&'static str> {
    use syn::BinOp;
    match op {
        BinOp::Add(_) | BinOp::Sub(_) | BinOp::Mul(_) | BinOp::Div(_) | BinOp::Rem(_) => {
            Some("arithmetic")
        }
        BinOp::And(_) | BinOp::Or(_) => Some("boolean_op"),
        BinOp::Eq(_) | BinOp::Ne(_) | BinOp::Lt(_) | BinOp::Le(_) | BinOp::Gt(_) | BinOp::Ge(_) => {
            Some("comparison")
        }
        BinOp::BitAnd(_) | BinOp::BitOr(_) | BinOp::BitXor(_) | BinOp::Shl(_) | BinOp::Shr(_) => {
            Some("bitwise")
        }
        _ => None,
    }
}
