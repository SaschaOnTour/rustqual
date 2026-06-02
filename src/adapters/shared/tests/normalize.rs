use crate::adapters::shared::normalize::*;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use syn::visit::Visit;

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

/// Structural hash of a function body parsed from `code`.
fn hash_of(code: &str) -> u64 {
    structural_hash(&normalize_body(&parse_body(code)))
}

#[test]
fn structural_hash_equality_by_structure() {
    // The structural hash ignores identifier names (so renamed-but-isomorphic
    // bodies collide) and is deterministic, but distinguishes operators, bool
    // literals, and overall shape. (label, code_a, code_b, same_hash)
    let cases: &[(&str, &str, &str, bool)] = &[
        (
            "same structure, different names → same hash",
            "let x = a + b;",
            "let y = p + q;",
            true,
        ),
        (
            "different operator → different hash",
            "let x = a + b;",
            "let x = a * b;",
            false,
        ),
        (
            "deterministic: same body hashes equal",
            "let x = foo(a, b);",
            "let x = foo(a, b);",
            true,
        ),
        (
            "bool true vs false → different hash",
            "return true;",
            "return false;",
            false,
        ),
        (
            "complex isomorphic bodies (renamed) → same hash",
            "for item in items { if item.is_valid() { results.push(item.name()); } }",
            "for entry in data { if entry.is_valid() { output.push(entry.name()); } }",
            true,
        ),
    ];
    for (label, code_a, code_b, same) in cases {
        let (a, b) = (hash_of(code_a), hash_of(code_b));
        assert_eq!(a == b, *same, "case {label}: hash_a={a}, hash_b={b}");
    }
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
