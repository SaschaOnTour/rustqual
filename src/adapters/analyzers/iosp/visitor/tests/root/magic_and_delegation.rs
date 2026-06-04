use super::*;

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
