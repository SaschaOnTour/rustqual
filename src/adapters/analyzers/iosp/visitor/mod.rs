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
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::adapters::analyzers::iosp::scope::ProjectScope;
    use crate::config::Config;
    use syn::visit::Visit;

    fn empty_scope() -> ProjectScope {
        ProjectScope::default()
    }

    #[test]
    fn test_new_defaults() {
        let config = Config::default();
        let scope = empty_scope();
        let visitor = BodyVisitor::new(&config, &scope, Some("test_fn"), None, HashMap::new());
        assert!(visitor.logic.is_empty());
        assert!(visitor.own_calls.is_empty());
        assert_eq!(visitor.max_nesting, 0);
        assert_eq!(visitor.closure_depth, 0);
        assert_eq!(visitor.async_block_depth, 0);
        assert_eq!(visitor.nesting_depth, 0);
        assert_eq!(visitor.current_fn_name, Some("test_fn".to_string()));
        assert_eq!(visitor.cognitive_complexity, 0);
        assert_eq!(visitor.cyclomatic_complexity, 1); // base path
        assert!(visitor.complexity_hotspots.is_empty());
        assert!(visitor.magic_numbers.is_empty());
        assert!(visitor.last_boolean_op.is_none());
        assert_eq!(visitor.in_const_context, 0);
    }

    #[test]
    fn test_new_without_fn_name() {
        let config = Config::default();
        let scope = empty_scope();
        let visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
        assert!(visitor.current_fn_name.is_none());
    }

    #[test]
    fn test_in_lenient_nested_context_closure() {
        let config = Config::default();
        let scope = empty_scope();
        let mut visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
        assert!(!visitor.in_lenient_nested_context());
        visitor.closure_depth = 1;
        assert!(visitor.in_lenient_nested_context());
    }

    #[test]
    fn test_in_lenient_nested_context_strict_mode() {
        let mut config = Config::default();
        config.strict_closures = true;
        let scope = empty_scope();
        let mut visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
        visitor.closure_depth = 1;
        assert!(!visitor.in_lenient_nested_context());
    }

    #[test]
    fn test_in_lenient_nested_context_async_block() {
        let config = Config::default();
        let scope = empty_scope();
        let mut visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
        visitor.async_block_depth = 1;
        assert!(visitor.in_lenient_nested_context());
    }

    #[test]
    fn test_is_iterator_method_known() {
        assert!(BodyVisitor::is_iterator_method("map"));
        assert!(BodyVisitor::is_iterator_method("filter"));
        assert!(BodyVisitor::is_iterator_method("collect"));
        assert!(BodyVisitor::is_iterator_method("fold"));
        assert!(BodyVisitor::is_iterator_method("iter"));
        assert!(BodyVisitor::is_iterator_method("into_iter"));
    }

    #[test]
    fn test_is_iterator_method_unknown() {
        assert!(!BodyVisitor::is_iterator_method("foo"));
        assert!(!BodyVisitor::is_iterator_method("bar"));
        assert!(!BodyVisitor::is_iterator_method("push"));
        assert!(!BodyVisitor::is_iterator_method("analyze"));
    }

    #[test]
    fn test_is_recursive_call_match() {
        let config = Config::default();
        let scope = empty_scope();
        let visitor = BodyVisitor::new(&config, &scope, Some("my_func"), None, HashMap::new());
        assert!(visitor.is_recursive_call("my_func"));
    }

    #[test]
    fn test_is_recursive_call_qualified() {
        let config = Config::default();
        let scope = empty_scope();
        let visitor = BodyVisitor::new(&config, &scope, Some("bar"), None, HashMap::new());
        assert!(visitor.is_recursive_call("Foo::bar"));
    }

    #[test]
    fn test_is_recursive_call_no_fn_name() {
        let config = Config::default();
        let scope = empty_scope();
        let visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
        assert!(!visitor.is_recursive_call("anything"));
    }

    #[test]
    fn test_enter_exit_nesting() {
        let config = Config::default();
        let scope = empty_scope();
        let mut visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
        assert_eq!(visitor.nesting_depth, 0);
        assert_eq!(visitor.max_nesting, 0);

        visitor.enter_nesting();
        assert_eq!(visitor.nesting_depth, 1);
        assert_eq!(visitor.max_nesting, 1);

        visitor.enter_nesting();
        assert_eq!(visitor.nesting_depth, 2);
        assert_eq!(visitor.max_nesting, 2);

        visitor.exit_nesting();
        assert_eq!(visitor.nesting_depth, 1);
        assert_eq!(visitor.max_nesting, 2);

        visitor.exit_nesting();
        assert_eq!(visitor.nesting_depth, 0);
        assert_eq!(visitor.max_nesting, 2);
    }

    #[test]
    fn test_extract_call_name_path() {
        let expr: syn::Expr = syn::parse_quote!(foo::bar);
        assert_eq!(
            BodyVisitor::extract_call_name(&expr),
            Some("foo::bar".to_string())
        );
    }

    #[test]
    fn test_extract_call_name_simple() {
        let expr: syn::Expr = syn::parse_quote!(my_func);
        assert_eq!(
            BodyVisitor::extract_call_name(&expr),
            Some("my_func".to_string())
        );
    }

    #[test]
    fn test_extract_call_name_non_path() {
        let expr: syn::Expr = syn::parse_quote!(42);
        assert_eq!(BodyVisitor::extract_call_name(&expr), None);
    }

    #[test]
    fn test_record_logic_normal() {
        let config = Config::default();
        let scope = empty_scope();
        let mut visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
        visitor.record_logic("if", proc_macro2::Span::call_site());
        assert_eq!(visitor.logic.len(), 1);
        assert_eq!(visitor.logic[0].kind, "if");
    }

    #[test]
    fn test_record_logic_skipped_in_closure() {
        let config = Config::default();
        let scope = empty_scope();
        let mut visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
        visitor.closure_depth = 1;
        visitor.record_logic("if", proc_macro2::Span::call_site());
        assert!(visitor.logic.is_empty());
    }

    #[test]
    fn test_record_logic_in_for_iter() {
        let config = Config::default();
        let scope = empty_scope();
        let mut visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
        visitor.in_for_iter = true;
        visitor.record_logic("comparison", proc_macro2::Span::call_site());
        assert!(visitor.logic.is_empty());
    }

    #[test]
    fn test_record_logic_in_async_block_lenient() {
        let config = Config::default();
        let scope = empty_scope();
        let mut visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
        visitor.async_block_depth = 1;
        visitor.record_logic("if", proc_macro2::Span::call_site());
        assert!(visitor.logic.is_empty());
    }

    // ── Complexity tracking tests ─────────────────────────────────────

    fn visit_code(code: &str) -> BodyVisitor<'static> {
        // Leak config and scope to satisfy lifetime requirements
        let config: &'static Config = Box::leak(Box::default());
        let scope: &'static ProjectScope = Box::leak(Box::default());
        let mut visitor = BodyVisitor::new(config, scope, Some("test_fn"), None, HashMap::new());
        let block: syn::Block = syn::parse_str(&format!("{{ {code} }}")).unwrap();
        block.stmts.iter().for_each(|stmt| visitor.visit_stmt(stmt));
        visitor
    }

    #[test]
    fn test_cognitive_simple_if() {
        let v = visit_code("if true { let _ = 1; }");
        // if at nesting 0: 1 + 0 = 1
        assert_eq!(v.cognitive_complexity, 1);
    }

    #[test]
    fn test_cognitive_nested_if() {
        let v = visit_code("if true { if false { let _ = 1; } }");
        // outer if at nesting 0: 1+0=1, inner if at nesting 1: 1+1=2, total=3
        assert_eq!(v.cognitive_complexity, 3);
    }

    #[test]
    fn test_cognitive_deep_nesting() {
        let v = visit_code("if true { if false { if true { let _ = 1; } } }");
        // nesting 0: 1, nesting 1: 2, nesting 2: 3, total=6
        assert_eq!(v.cognitive_complexity, 6);
    }

    #[test]
    fn test_cognitive_match() {
        let v = visit_code("match 1 { 1 => {}, 2 => {}, _ => {} }");
        // match at nesting 0: 1+0=1
        assert_eq!(v.cognitive_complexity, 1);
    }

    #[test]
    fn test_cognitive_for_while_loop() {
        let v = visit_code("for _ in 0..10 { while true { loop { break; } } }");
        // for at 0: 1, while at 1: 2, loop at 2: 3, total=6
        assert_eq!(v.cognitive_complexity, 6);
    }

    #[test]
    fn test_cognitive_boolean_alternation() {
        // a && b || c should have 1 alternation
        let v = visit_code("let _ = true && false || true;");
        // The alternation adds 1 to cognitive
        assert!(
            v.cognitive_complexity >= 1,
            "Expected alternation to add to cognitive, got {}",
            v.cognitive_complexity
        );
    }

    #[test]
    fn test_cognitive_no_alternation() {
        let v = visit_code("let _ = true && false && true;");
        // No alternation — same operator throughout
        // cognitive stays 0 for boolean ops (no alternation)
        assert_eq!(v.cognitive_complexity, 0);
    }

    #[test]
    fn test_cyclomatic_basic() {
        let v = visit_code("if true {} if false {}");
        // base=1, +1 per if = 3
        assert_eq!(v.cyclomatic_complexity, 3);
    }

    #[test]
    fn test_cyclomatic_match_arms_all_trivial() {
        // All arms return literals → all trivial → cyclomatic = base only
        let v = visit_code("match x { 1 => \"a\", 2 => \"b\", 3 => \"c\", _ => \"d\" }");
        // base=1, all 4 arms are trivial (Lit bodies) → +0
        assert_eq!(v.cyclomatic_complexity, 1);
    }

    #[test]
    fn test_cyclomatic_match_arms_with_control_flow() {
        // Arms with if/match bodies are non-trivial
        let v = visit_code("match x { 1 => if y { 1 } else { 2 }, 2 => 0, _ => 0 }");
        // base=1, 1 non-trivial arm (if body) → +(1-1)=0, plus inner if: +1
        assert_eq!(v.cyclomatic_complexity, 2);
    }

    #[test]
    fn test_cyclomatic_match_all_nontrivial() {
        // All arms have blocks with multiple stmts → non-trivial
        let v = visit_code(
            "match x { 1 => { let a = 1; a }, 2 => { let b = 2; b }, _ => { let c = 3; c } }",
        );
        // base=1, 3 non-trivial arms → +(3-1)=2
        assert_eq!(v.cyclomatic_complexity, 3);
    }

    #[test]
    fn test_cyclomatic_lookup_table_match() {
        // Large lookup table like bin_op_str — all literal returns
        let v = visit_code(
            r#"match op {
            0 => "+", 1 => "-", 2 => "*", 3 => "/",
            4 => "%", 5 => "&&", 6 => "||", 7 => "^",
            8 => "&", 9 => "|", _ => "?"
        }"#,
        );
        // base=1, all 11 arms trivial (Lit) → +0
        assert_eq!(v.cyclomatic_complexity, 1);
    }

    #[test]
    fn test_cyclomatic_match_mixed_trivial_nontrivial() {
        // Mix of trivial and non-trivial arms
        let v = visit_code(
            "match x { 1 => true, 2 => if y { true } else { false }, 3 => false, _ => false }",
        );
        // base=1, 1 non-trivial arm (if body) → +(1-1)=0, plus inner if: +1
        assert_eq!(v.cyclomatic_complexity, 2);
    }

    #[test]
    fn test_cyclomatic_boolean_ops() {
        let v = visit_code("let _ = true && false || true;");
        // base=1, +1 for &&, +1 for || = 3
        assert_eq!(v.cyclomatic_complexity, 3);
    }

    #[test]
    fn test_complexity_hotspot_at_deep_nesting() {
        let v = visit_code("if true { if false { if true { if false { let _ = 1; } } } }");
        // nesting reaches 3 at the 4th if → hotspot recorded
        assert!(
            !v.complexity_hotspots.is_empty(),
            "Expected hotspot at nesting >= 3"
        );
        assert_eq!(v.complexity_hotspots[0].nesting_depth, 3);
        assert_eq!(v.complexity_hotspots[0].construct, "if");
    }

    #[test]
    fn test_no_hotspot_at_shallow_nesting() {
        let v = visit_code("if true { if false { let _ = 1; } }");
        // max nesting is 2, below threshold of 3
        assert!(v.complexity_hotspots.is_empty());
    }

    // ── Magic number detection tests ──────────────────────────────────

    #[test]
    fn test_magic_number_detected() {
        let v = visit_code("let x = 42;");
        assert_eq!(v.magic_numbers.len(), 1);
        assert_eq!(v.magic_numbers[0].value, "42");
    }

    #[test]
    fn test_magic_number_allowed_not_flagged() {
        let v = visit_code("let x = 0; let y = 1; let z = 2;");
        // 0, 1, 2 are in the default allowed list
        assert!(v.magic_numbers.is_empty());
    }

    #[test]
    fn test_magic_number_negative_detected() {
        let v = visit_code("let x = -42;");
        assert_eq!(v.magic_numbers.len(), 1);
        assert_eq!(v.magic_numbers[0].value, "-42");
    }

    #[test]
    fn test_magic_number_negative_one_allowed() {
        let v = visit_code("let x = -1;");
        // -1 is in the default allowed list
        assert!(v.magic_numbers.is_empty());
    }

    #[test]
    fn test_magic_number_float_detected() {
        let v = visit_code("let x = 3.14;");
        assert_eq!(v.magic_numbers.len(), 1);
        assert_eq!(v.magic_numbers[0].value, "3.14");
    }

    #[test]
    fn test_magic_number_in_const_not_flagged() {
        let v = visit_code("const LIMIT: i32 = 42;");
        assert!(
            v.magic_numbers.is_empty(),
            "Const context should suppress magic numbers, got {:?}",
            v.magic_numbers
        );
    }

    #[test]
    fn test_magic_number_detection_disabled() {
        let mut config = Config::default();
        config.complexity.detect_magic_numbers = false;
        let scope = empty_scope();
        let mut visitor = BodyVisitor::new(&config, &scope, Some("test_fn"), None, HashMap::new());
        let block: syn::Block = syn::parse_str("{ let x = 42; }").unwrap();
        block.stmts.iter().for_each(|stmt| visitor.visit_stmt(stmt));
        assert!(visitor.magic_numbers.is_empty());
    }

    // ── Delegation detection tests ───────────────────────────────────

    #[test]
    fn test_delegation_single_call() {
        let block: syn::Block = syn::parse_str("{ call(x); }").unwrap();
        assert!(is_delegation_only_body(&block.stmts));
    }

    #[test]
    fn test_delegation_method_call_with_try() {
        let block: syn::Block = syn::parse_str("{ wtr.write_record(f(t))?; }").unwrap();
        assert!(is_delegation_only_body(&block.stmts));
    }

    #[test]
    fn test_delegation_await() {
        let block: syn::Block = syn::parse_str("{ sync(s).await; }").unwrap();
        assert!(is_delegation_only_body(&block.stmts));
    }

    #[test]
    fn test_delegation_if_let_push() {
        let block: syn::Block =
            syn::parse_str("{ if let Some(r) = call()? { v.push(r); } }").unwrap();
        assert!(is_delegation_only_body(&block.stmts));
    }

    #[test]
    fn test_delegation_let_binding() {
        let block: syn::Block = syn::parse_str("{ let r = call(x); store(r); }").unwrap();
        assert!(is_delegation_only_body(&block.stmts));
    }

    #[test]
    fn test_delegation_multiple_calls() {
        let block: syn::Block = syn::parse_str("{ a(x); b(y); }").unwrap();
        assert!(is_delegation_only_body(&block.stmts));
    }

    #[test]
    fn test_not_delegation_comparison() {
        let block: syn::Block = syn::parse_str("{ if x > 0 { call(x); } }").unwrap();
        assert!(!is_delegation_only_body(&block.stmts));
    }

    #[test]
    fn test_not_delegation_arithmetic() {
        let block: syn::Block = syn::parse_str("{ let y = x + 1; call(y); }").unwrap();
        assert!(!is_delegation_only_body(&block.stmts));
    }

    #[test]
    fn test_not_delegation_match() {
        let block: syn::Block =
            syn::parse_str("{ match x { 0 => call_a(), _ => call_b() } }").unwrap();
        assert!(!is_delegation_only_body(&block.stmts));
    }

    // ---------------------------------------------------------------
    // Match-Dispatch Detection
    // ---------------------------------------------------------------

    fn parse_match_arms(code: &str) -> Vec<syn::Arm> {
        let expr: syn::ExprMatch = syn::parse_str(code).unwrap();
        expr.arms
    }

    #[test]
    fn test_match_dispatch_all_calls() {
        let arms = parse_match_arms("match x { 0 => call_a(), _ => call_b() }");
        assert!(is_match_dispatch(&arms));
    }

    #[test]
    fn test_match_dispatch_method_calls() {
        let arms = parse_match_arms("match x { A => self.run_a(d), B => self.run_b(d) }");
        assert!(is_match_dispatch(&arms));
    }

    #[test]
    fn test_match_dispatch_with_try() {
        let arms = parse_match_arms("match x { 0 => call_a()?, _ => call_b()? }");
        assert!(is_match_dispatch(&arms));
    }

    #[test]
    fn test_match_dispatch_block_with_call() {
        let arms = parse_match_arms("match x { 0 => { call_a() }, _ => { call_b() } }");
        assert!(is_match_dispatch(&arms));
    }

    #[test]
    fn test_match_not_dispatch_logic_in_arm() {
        let arms = parse_match_arms("match x { 0 => { let d = call(); d + 1 }, _ => call_b() }");
        assert!(!is_match_dispatch(&arms));
    }

    #[test]
    fn test_match_not_dispatch_with_guard() {
        let arms = parse_match_arms("match x { n if n > 0 => call_a(), _ => call_b() }");
        assert!(!is_match_dispatch(&arms));
    }

    #[test]
    fn test_match_not_dispatch_arithmetic() {
        let arms = parse_match_arms("match x { 0 => a + b, _ => call_b() }");
        assert!(!is_match_dispatch(&arms));
    }

    #[test]
    fn test_match_dispatch_tuple_pattern() {
        let arms =
            parse_match_arms("match (a, b) { (Some(_), Some(p)) => call_a(p), _ => call_b() }");
        assert!(is_match_dispatch(&arms));
    }

    // ── Array index magic number exclusion ───────────────────────────

    #[test]
    fn test_magic_number_in_array_index_not_flagged() {
        let v = visit_code("let x = arr[3];");
        assert!(
            v.magic_numbers.is_empty(),
            "Array index 3 should not be flagged"
        );
    }

    #[test]
    fn test_magic_number_outside_index_still_flagged() {
        let v = visit_code("let x = arr[3]; let y = 42;");
        assert_eq!(v.magic_numbers.len(), 1);
        assert_eq!(v.magic_numbers[0].value, "42");
    }

    #[test]
    fn test_magic_number_nested_index_not_flagged() {
        let v = visit_code("let x = matrix[3][4];");
        // Only the index expressions (3, 4) should be suppressed; no other magic numbers
        let flagged: Vec<&str> = v.magic_numbers.iter().map(|m| m.value.as_str()).collect();
        assert!(
            flagged.is_empty(),
            "Nested array indices should not be flagged, got: {flagged:?}"
        );
    }
}
