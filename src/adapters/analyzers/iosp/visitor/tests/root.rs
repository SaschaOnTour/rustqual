use crate::adapters::analyzers::iosp::scope::ProjectScope;
use crate::adapters::analyzers::iosp::visitor::*;
use crate::config::Config;
use std::collections::HashMap;
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
    let block: syn::Block = syn::parse_str("{ if let Some(r) = call()? { v.push(r); } }").unwrap();
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
    let block: syn::Block = syn::parse_str("{ match x { 0 => call_a(), _ => call_b() } }").unwrap();
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
    let arms = parse_match_arms("match (a, b) { (Some(_), Some(p)) => call_a(p), _ => call_b() }");
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
