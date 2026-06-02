use super::*;

// ── Complexity tracking tests ─────────────────────────────────────

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
