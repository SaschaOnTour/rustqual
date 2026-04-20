mod visit;

use std::collections::HashMap;

use crate::adapters::analyzers::iosp::scope::ProjectScope;
use crate::config::Config;

use super::types::{CallOccurrence, ComplexityHotspot, LogicOccurrence, MagicNumberOccurrence};

/// Nesting depth at which a complexity hotspot is recorded.
pub(crate) const HOTSPOT_NESTING_DEPTH: usize = 3;

/// Visitor that collects all logic and call occurrences inside a function body.
// qual:allow(srp) reason: "syn visitor pattern — all fields serve the single visit_expr traversal"
pub(crate) struct BodyVisitor<'a> {
    pub config: &'a Config,
    pub scope: &'a ProjectScope,
    pub logic: Vec<LogicOccurrence>,
    pub own_calls: Vec<CallOccurrence>,
    /// Current nesting depth for closures (to optionally skip them)
    pub(super) closure_depth: usize,
    /// Whether we're inside a for-loop's iterator expression (skip logic there)
    pub(super) in_for_iter: bool,
    /// Current nesting depth for logic constructs (if/match/for/while/loop)
    pub(super) nesting_depth: usize,
    /// Maximum nesting depth observed
    pub max_nesting: usize,
    /// Current function name (for recursion detection)
    pub(super) current_fn_name: Option<String>,
    /// Whether we're inside an async block
    pub(super) async_block_depth: usize,
    /// Cognitive complexity score (SonarSource-style).
    pub cognitive_complexity: usize,
    /// Cyclomatic complexity score (starts at 1 for the base path).
    pub cyclomatic_complexity: usize,
    /// Locations with deep nesting contributing to complexity.
    pub complexity_hotspots: Vec<ComplexityHotspot>,
    /// Magic number literals found in non-const context.
    pub magic_numbers: Vec<MagicNumberOccurrence>,
    /// Last boolean operator seen (true=And, false=Or) for alternation tracking.
    pub(super) last_boolean_op: Option<bool>,
    /// Depth counter for const/static context (magic numbers are not flagged here).
    pub(super) in_const_context: usize,
    /// Depth counter for array index context (magic numbers are not flagged here).
    pub(super) in_index_context: usize,
    /// Number of unsafe blocks encountered.
    pub unsafe_block_count: usize,
    /// Number of `.unwrap()` calls encountered (all contexts including closures).
    pub unwrap_count: usize,
    /// Number of `.expect()` calls encountered (all contexts including closures).
    pub expect_count: usize,
    /// Number of `panic!` / `unreachable!` macro invocations (all contexts).
    pub panic_count: usize,
    /// Number of `todo!` macro invocations (all contexts).
    pub todo_count: usize,
    /// Current impl type (for self.method() resolution).
    pub(super) parent_type: Option<String>,
    /// Parameter name → type name mapping (for receiver type resolution).
    pub(super) param_types: HashMap<String, String>,
}

impl<'a> BodyVisitor<'a> {
    pub fn new(
        config: &'a Config,
        scope: &'a ProjectScope,
        fn_name: Option<&str>,
        parent_type: Option<&str>,
        param_types: HashMap<String, String>,
    ) -> Self {
        Self {
            config,
            scope,
            logic: Vec::new(),
            own_calls: Vec::new(),
            closure_depth: 0,
            in_for_iter: false,
            nesting_depth: 0,
            max_nesting: 0,
            current_fn_name: fn_name.map(String::from),
            async_block_depth: 0,
            cognitive_complexity: 0,
            cyclomatic_complexity: 1, // base path
            complexity_hotspots: Vec::new(),
            magic_numbers: Vec::new(),
            last_boolean_op: None,
            in_const_context: 0,
            in_index_context: 0,
            unsafe_block_count: 0,
            unwrap_count: 0,
            expect_count: 0,
            panic_count: 0,
            todo_count: 0,
            parent_type: parent_type.map(String::from),
            param_types,
        }
    }

    /// Resolve the type of a method call receiver expression.
    /// Returns Some(type_name) for `self` and simple parameter identifiers.
    /// Operation: pattern matching logic, no own calls.
    pub(super) fn resolve_receiver_type(&self, receiver: &syn::Expr) -> Option<&str> {
        match receiver {
            syn::Expr::Path(p) if p.path.is_ident("self") => self.parent_type.as_deref(),
            syn::Expr::Path(p) => {
                let ident = p.path.get_ident()?.to_string();
                self.param_types.get(&ident).map(|s| s.as_str())
            }
            _ => None,
        }
    }

    /// Check if a method call is an own call using type-aware resolution.
    /// Layer 1: receiver type known → check type's methods.
    /// Layer 2 fallback: receiver type unknown → check scope (conservative).
    /// Operation: conditional logic, no own calls.
    pub(super) fn is_type_resolved_own_method(&self, method: &str, receiver: &syn::Expr) -> bool {
        self.resolve_receiver_type(receiver)
            .map(|rt| self.scope.is_own_self_method(method, rt))
            .unwrap_or_else(|| self.scope.is_own_method(method))
    }

    /// Check if we're inside a closure or async block in lenient mode.
    /// Operation: boolean logic.
    pub(super) fn in_lenient_nested_context(&self) -> bool {
        !self.config.strict_closures && (self.closure_depth > 0 || self.async_block_depth > 0)
    }

    /// Record a logic occurrence, respecting closure depth in lenient mode.
    /// Operation: contains logic (if, &&) but no own calls.
    pub(super) fn record_logic(&mut self, kind: &str, span: proc_macro2::Span) {
        if !self.config.strict_closures && self.closure_depth > 0 {
            return;
        }
        if self.in_for_iter {
            return;
        }
        // Async blocks are treated like closures in lenient mode
        if self.async_block_depth > 0 && !self.config.strict_closures {
            return;
        }
        self.logic.push(LogicOccurrence {
            kind: kind.to_string(),
            line: span.start().line,
        });
    }

    pub(super) fn extract_call_name(expr: &syn::Expr) -> Option<String> {
        match expr {
            syn::Expr::Path(syn::ExprPath { path, .. }) => Some(
                path.segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::"),
            ),
            _ => None,
        }
    }

    pub(super) fn is_iterator_method(method_name: &str) -> bool {
        matches!(
            method_name,
            "map"
                | "filter"
                | "filter_map"
                | "flat_map"
                | "for_each"
                | "fold"
                | "reduce"
                | "any"
                | "all"
                | "find"
                | "find_map"
                | "position"
                | "skip"
                | "take"
                | "skip_while"
                | "take_while"
                | "zip"
                | "enumerate"
                | "chain"
                | "inspect"
                | "partition"
                | "scan"
                | "peekable"
                | "sum"
                | "product"
                | "count"
                | "min"
                | "max"
                | "min_by"
                | "max_by"
                | "min_by_key"
                | "max_by_key"
                | "collect"
                | "iter"
                | "into_iter"
                | "iter_mut"
        )
    }

    /// Check if a call name is a recursive call to the current function.
    /// Operation: comparison logic.
    pub(super) fn is_recursive_call(&self, name: &str) -> bool {
        if let Some(ref fn_name) = self.current_fn_name {
            name == fn_name || name.ends_with(&format!("::{fn_name}"))
        } else {
            false
        }
    }

    /// Track nesting depth entry.
    pub(super) fn enter_nesting(&mut self) {
        self.nesting_depth += 1;
        if self.nesting_depth > self.max_nesting {
            self.max_nesting = self.nesting_depth;
        }
    }

    /// Track nesting depth exit.
    pub(super) fn exit_nesting(&mut self) {
        self.nesting_depth -= 1;
    }

    /// Record a magic number if detection is enabled and value is not in allowed list.
    /// Operation: string comparison logic, no own calls.
    pub(super) fn record_magic_number(&mut self, value: String, span: proc_macro2::Span) {
        if !self.config.complexity.detect_magic_numbers {
            return;
        }
        if self.in_const_context > 0 || self.in_index_context > 0 {
            return;
        }
        if self
            .config
            .complexity
            .allowed_magic_numbers
            .iter()
            .any(|a| a == &value)
        {
            return;
        }
        self.magic_numbers.push(MagicNumberOccurrence {
            line: span.start().line,
            value,
        });
    }

    /// Record a complexity hotspot if nesting is deep enough.
    /// Operation: comparison + push logic.
    pub(super) fn record_hotspot(&mut self, construct: &str, span: proc_macro2::Span) {
        if self.nesting_depth >= HOTSPOT_NESTING_DEPTH {
            self.complexity_hotspots.push(ComplexityHotspot {
                line: span.start().line,
                nesting_depth: self.nesting_depth,
                construct: construct.to_string(),
            });
        }
    }
}

/// Extract expressions from statements for delegation checking.
/// Returns `None` if any statement is not a valid delegation statement.
/// Operation: loop + match + if-let logic, no own calls.
fn extract_delegation_exprs(stmts: &[syn::Stmt]) -> Option<Vec<&syn::Expr>> {
    let mut out = Vec::new();
    for s in stmts {
        match s {
            syn::Stmt::Expr(e, _) => out.push(e),
            syn::Stmt::Local(l) => {
                if let Some(init) = &l.init {
                    out.push(&init.expr);
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

/// Check if all expressions in the initial set are delegation-only.
/// Uses iterative stack-based traversal with `let...else` for flat control flow.
/// Operation: loop + match logic, no own calls (extract called via closure).
fn check_delegation_stack(initial: Vec<&syn::Expr>) -> bool {
    let mut stack = initial;
    let extract: fn(&[syn::Stmt]) -> Option<Vec<&syn::Expr>> = extract_delegation_exprs;
    while let Some(e) = stack.pop() {
        match e {
            syn::Expr::Call(_) | syn::Expr::MethodCall(_) => {}
            syn::Expr::Try(t) => stack.push(&t.expr),
            syn::Expr::Await(a) => stack.push(&a.base),
            syn::Expr::Return(r) => {
                if let Some(val) = &r.expr {
                    stack.push(val);
                }
            }
            syn::Expr::Break(_) | syn::Expr::Continue(_) | syn::Expr::Path(_) => {}
            syn::Expr::Paren(p) => stack.push(&p.expr),
            syn::Expr::Block(b) => {
                let Some(v) = extract(&b.block.stmts) else {
                    return false;
                };
                stack.extend(v);
            }
            syn::Expr::If(if_expr) => {
                let syn::Expr::Let(let_expr) = &*if_expr.cond else {
                    return false;
                };
                stack.push(&let_expr.expr);
                let Some(v) = extract(&if_expr.then_branch.stmts) else {
                    return false;
                };
                stack.extend(v);
                if let Some((_, else_expr)) = &if_expr.else_branch {
                    stack.push(else_expr);
                }
            }
            _ => return false,
        }
    }
    true
}

/// Check if all statements in a block body are delegation-only (calls with no real logic).
/// A for-loop with a delegation-only body is equivalent to `.for_each()` in lenient mode.
/// Only IOSP logic recording is affected — complexity metrics are always tracked.
/// Integration: orchestrates extract_delegation_exprs and check_delegation_stack.
pub(super) fn is_delegation_only_body(stmts: &[syn::Stmt]) -> bool {
    extract_delegation_exprs(stmts).is_some_and(|exprs| check_delegation_stack(exprs))
}

/// Check if a match expression is pure dispatch (all arms delegation-only, no guards).
/// A match-dispatch is equivalent to routing/dispatching — it's conceptually an Integration.
/// Only IOSP logic recording is affected — complexity metrics are always tracked.
/// Operation: iterator + boolean logic, no own calls (check_delegation_stack via closure).
pub(super) fn is_match_dispatch(arms: &[syn::Arm]) -> bool {
    let check = |body: &syn::Expr| check_delegation_stack(vec![body]);
    arms.iter()
        .all(|arm| arm.guard.is_none() && check(&arm.body))
}

/// Check if a match arm body is trivial (no nested control flow).
/// Trivial arms: direct data expressions without branching (literals, paths, calls, etc.).
/// Non-trivial: if, match, for, while, loop, blocks with complex statements.
/// Operation: pure pattern-matching, no own calls.
pub(super) fn is_trivial_match_arm(arm: &syn::Arm) -> bool {
    match &*arm.body {
        syn::Expr::Lit(_)
        | syn::Expr::Path(_)
        | syn::Expr::Field(_)
        | syn::Expr::Call(_)
        | syn::Expr::MethodCall(_)
        | syn::Expr::Tuple(_)
        | syn::Expr::Struct(_)
        | syn::Expr::Reference(_)
        | syn::Expr::Unary(_)
        | syn::Expr::Index(_)
        | syn::Expr::Cast(_) => true,
        // Block with single simple statement
        syn::Expr::Block(b) if b.block.stmts.len() == 1 => {
            matches!(&b.block.stmts[0],
                syn::Stmt::Expr(e, _) if matches!(e,
                    syn::Expr::Lit(_) | syn::Expr::Path(_) | syn::Expr::Call(_)
                    | syn::Expr::MethodCall(_) | syn::Expr::Field(_)
                    | syn::Expr::Reference(_) | syn::Expr::Unary(_)
                    | syn::Expr::Tuple(_) | syn::Expr::Struct(_)
                    | syn::Expr::Index(_) | syn::Expr::Cast(_)
                )
            )
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests;
