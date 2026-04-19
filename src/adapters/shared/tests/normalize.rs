use crate::adapters::shared::normalize::*;

/// Parse a function body from source code.
fn parse_body(code: &str) -> syn::Block {
    let wrapped = format!("fn test_fn() {{ {} }}", code);
    let file = syn::parse_file(&wrapped).expect("parse failed");
    let syn::Item::Fn(f) = &file.items[0] else {
        unreachable!("wrapped code is always a function")
    };
    *f.block.clone()
}

#[test]
fn test_normalize_empty_body() {
    let body = parse_body("");
    let tokens = normalize_body(&body);
    assert!(tokens.is_empty());
}

#[test]
fn test_normalize_let_binding() {
    let body = parse_body("let x = 1;");
    let tokens = normalize_body(&body);
    assert!(tokens.contains(&NormalizedToken::Keyword("let")));
    assert!(tokens.contains(&NormalizedToken::Ident(0)));
    assert!(tokens.contains(&NormalizedToken::IntLit));
    assert!(tokens.contains(&NormalizedToken::Semi));
}

#[test]
fn test_normalize_same_structure_different_names_same_hash() {
    let body_a = parse_body("let x = a + b;");
    let body_b = parse_body("let y = p + q;");
    let hash_a = structural_hash(&normalize_body(&body_a));
    let hash_b = structural_hash(&normalize_body(&body_b));
    assert_eq!(hash_a, hash_b);
}

#[test]
fn test_normalize_different_structure_different_hash() {
    let body_a = parse_body("let x = a + b;");
    let body_b = parse_body("let x = a * b;");
    let hash_a = structural_hash(&normalize_body(&body_a));
    let hash_b = structural_hash(&normalize_body(&body_b));
    assert_ne!(hash_a, hash_b);
}

#[test]
fn test_structural_hash_deterministic() {
    let body = parse_body("let x = foo(a, b);");
    let hash1 = structural_hash(&normalize_body(&body));
    let hash2 = structural_hash(&normalize_body(&body));
    assert_eq!(hash1, hash2);
}

#[test]
fn test_jaccard_identical() {
    let body = parse_body("let x = 1;");
    let tokens = normalize_body(&body);
    let sim = jaccard_similarity(&tokens, &tokens);
    assert!((sim - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_jaccard_disjoint() {
    let a = vec![NormalizedToken::IntLit, NormalizedToken::Keyword("if")];
    let b = vec![NormalizedToken::StrLit, NormalizedToken::Keyword("for")];
    let sim = jaccard_similarity(&a, &b);
    assert!((sim).abs() < f64::EPSILON);
}

#[test]
fn test_jaccard_partial_overlap() {
    let a = vec![
        NormalizedToken::Keyword("let"),
        NormalizedToken::IntLit,
        NormalizedToken::Semi,
    ];
    let b = vec![
        NormalizedToken::Keyword("let"),
        NormalizedToken::StrLit,
        NormalizedToken::Semi,
    ];
    let sim = jaccard_similarity(&a, &b);
    // 2 shared (let, semi), 1 different each (IntLit vs StrLit)
    // intersection=2, union=4 → 0.5
    assert!((sim - 0.5).abs() < f64::EPSILON);
}

#[test]
fn test_jaccard_both_empty() {
    let sim = jaccard_similarity(&[], &[]);
    assert!((sim - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_jaccard_one_empty() {
    let a = vec![NormalizedToken::IntLit];
    let sim = jaccard_similarity(&a, &[]);
    assert!((sim).abs() < f64::EPSILON);
}

#[test]
fn test_normalize_if_expression() {
    let body = parse_body("if x > 0 { return true; }");
    let tokens = normalize_body(&body);
    assert!(tokens.contains(&NormalizedToken::Keyword("if")));
    assert!(tokens.contains(&NormalizedToken::Operator(">")));
    assert!(tokens.contains(&NormalizedToken::Keyword("return")));
    assert!(tokens.contains(&NormalizedToken::BoolLit(true)));
}

#[test]
fn test_normalize_method_call_preserves_name() {
    let body = parse_body("x.push(42);");
    let tokens = normalize_body(&body);
    assert!(tokens.contains(&NormalizedToken::MethodCall("push".to_string())));
    assert!(tokens.contains(&NormalizedToken::IntLit));
}

#[test]
fn test_normalize_field_access_preserves_name() {
    let body = parse_body("let v = self.name;");
    let tokens = normalize_body(&body);
    assert!(tokens.contains(&NormalizedToken::FieldAccess("name".to_string())));
}

#[test]
fn test_normalize_bool_values_distinct() {
    let body_true = parse_body("return true;");
    let body_false = parse_body("return false;");
    let hash_true = structural_hash(&normalize_body(&body_true));
    let hash_false = structural_hash(&normalize_body(&body_false));
    assert_ne!(hash_true, hash_false);
}

#[test]
fn test_normalize_stmts_subset() {
    let body = parse_body("let a = 1; let b = 2; let c = 3;");
    // Normalize only the first two statements
    let tokens_first_two = normalize_stmts(&body.stmts[..2]);
    let tokens_all = normalize_body(&body);
    assert!(tokens_first_two.len() < tokens_all.len());
    // Both start with the same prefix (same normalization)
    assert_eq!(tokens_first_two[..4], tokens_all[..4]);
}

#[test]
fn test_normalize_for_loop() {
    let body = parse_body("for item in list { process(item); }");
    let tokens = normalize_body(&body);
    assert!(tokens.contains(&NormalizedToken::Keyword("for")));
    assert!(tokens.contains(&NormalizedToken::Keyword("in")));
}

#[test]
fn test_normalize_match_expression() {
    let body = parse_body("match x { 0 => true, _ => false }");
    let tokens = normalize_body(&body);
    assert!(tokens.contains(&NormalizedToken::Keyword("match")));
    assert!(tokens.contains(&NormalizedToken::Operator("=>")));
    assert!(tokens.contains(&NormalizedToken::BoolLit(true)));
    assert!(tokens.contains(&NormalizedToken::BoolLit(false)));
    assert!(tokens.contains(&NormalizedToken::Keyword("_")));
}

#[test]
fn test_normalize_closure() {
    let body = parse_body("let f = |x| x + 1;");
    let tokens = normalize_body(&body);
    assert!(tokens.contains(&NormalizedToken::Keyword("closure")));
    assert!(tokens.contains(&NormalizedToken::Operator("+")));
}

#[test]
fn test_normalize_try_operator() {
    let body = parse_body("let r = foo()?;");
    let tokens = normalize_body(&body);
    assert!(tokens.contains(&NormalizedToken::Operator("?")));
}

#[test]
fn test_normalize_reference() {
    let body = parse_body("let r = &mut x;");
    let tokens = normalize_body(&body);
    assert!(tokens.contains(&NormalizedToken::Operator("&")));
    assert!(tokens.contains(&NormalizedToken::Keyword("mut")));
}

#[test]
fn test_normalize_macro_call() {
    let body = parse_body("println!(\"hello\");");
    let tokens = normalize_body(&body);
    assert!(tokens.contains(&NormalizedToken::MacroCall("println".to_string())));
}

#[test]
fn test_normalize_complex_same_structure() {
    // Two functions with same structure: iterate, check condition, push to vec
    let body_a =
        parse_body("for item in items { if item.is_valid() { results.push(item.name()); } }");
    let body_b =
        parse_body("for entry in data { if entry.is_valid() { output.push(entry.name()); } }");
    let hash_a = structural_hash(&normalize_body(&body_a));
    let hash_b = structural_hash(&normalize_body(&body_b));
    assert_eq!(hash_a, hash_b);
}
