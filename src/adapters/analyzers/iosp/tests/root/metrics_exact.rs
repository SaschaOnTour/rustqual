//! Per-construct metric assertions for `BodyVisitor`. Each fixture isolates a
//! single construct so the relevant counter is driven from its base — counters
//! base at 0 (a `> 0` check kills `*=` / arm-deletion, and `-=` underflow
//! panics), while cyclomatic bases at 1 so it needs an exact check. Shared
//! helpers (`metrics_of`, …) live in the parent module.
use super::*;

// Cyclomatic complexity starts at 1 (the base path), so each loop must take it
// to exactly 2 — an exact check is needed because `1 *= 1` stays 1 (a `> 0`
// check would not notice the lost increment).

#[test]
fn for_loop_increments_cyclomatic() {
    assert_eq!(
        metrics_of("fn f() { for _ in 0..2 { let _ = (); } }").cyclomatic_complexity,
        2
    );
}

#[test]
fn while_loop_increments_cyclomatic() {
    assert_eq!(
        metrics_of("fn f() { while false { let _ = (); } }").cyclomatic_complexity,
        2
    );
}

#[test]
fn loop_increments_cyclomatic() {
    assert_eq!(
        metrics_of("fn f() { loop { break; } }").cyclomatic_complexity,
        2
    );
}

#[test]
fn and_after_or_increments_cognitive() {
    // `a && b || c` = `(a && b) || c`: the outer Or remembers `is_and = false`,
    // then its left child `&&` differs → one cognitive jump on the *And* branch.
    // (The mirror Or branch is unreachable — an Or can never be visited while
    // the remembered op is And, since that needs a parenthesised Or as a `&&`
    // operand and the paren resets the tracker — so it is an equivalent mutant.)
    let m = metrics_of("fn f(a: bool, b: bool, c: bool) -> bool { a && b || c }");
    assert!(
        m.cognitive_complexity > 0,
        "an alternating && after an || is a cognitive jump, got {}",
        m.cognitive_complexity
    );
}

// The counters below sit on `BodyVisitor`, which only runs for *non-trivial*
// bodies (`is_trivial_body` short-circuits single-expression getters). Each
// fixture wraps the construct in an `if` so the visitor runs and `complexity`
// is `Some`, without otherwise touching the counter under test.

#[test]
fn unsafe_block_counted() {
    assert!(metrics_of("fn f(x: bool) { if x { unsafe { let _ = 1; } } }").unsafe_blocks > 0);
}

// The macros below are written in *expression* position (block tail, no
// trailing `;`): a `panic!();` statement is a `Stmt::Macro`, but the visitor's
// macro arm fires on `Expr::Macro`, which is the tail-expression form.

#[test]
fn panic_macro_counted() {
    assert!(metrics_of(r#"fn f(x: bool) { if x { panic!("boom") } }"#).panic_count > 0);
}

#[test]
fn unreachable_macro_counted_as_panic() {
    assert!(metrics_of("fn f(x: bool) { if x { unreachable!() } }").panic_count > 0);
}

#[test]
fn todo_macro_counted() {
    assert!(metrics_of("fn f(x: bool) { if x { todo!() } }").todo_count > 0);
}

#[test]
fn unwrap_counted() {
    assert!(
        metrics_of("fn f(o: Option<i32>, x: bool) -> i32 { if x { o.unwrap() } else { 0 } }")
            .unwrap_count
            > 0
    );
}

#[test]
fn expect_counted() {
    assert!(
        metrics_of(r#"fn f(o: Option<i32>, x: bool) -> i32 { if x { o.expect("e") } else { 0 } }"#)
            .expect_count
            > 0
    );
}

#[test]
fn comparison_counts_as_logic() {
    // Guards the comparison match arm (`==`/`!=`/`<`/…) of the logic detector.
    assert!(metrics_of("fn f(a: i32, b: i32) -> bool { a == b }").logic_count > 0);
}

#[test]
fn bitwise_counts_as_logic() {
    // Guards the bitwise match arm (`&`/`|`/`^`/`<<`/`>>`).
    assert!(metrics_of("fn f(a: i32, b: i32) -> i32 { a & b }").logic_count > 0);
}

#[test]
fn float_literal_is_a_magic_number() {
    // Guards the `Lit::Float` arm of magic-number detection.
    assert!(
        !metrics_of("fn f(x: bool) { if x { let _ = 3.14; } }")
            .magic_numbers
            .is_empty(),
        "a bare float literal is a magic number"
    );
}

#[test]
fn negative_float_literal_records_signed_value() {
    // `-3.14` is handled as a unit by the unary-negation arm, recording the
    // signed value. Guards that arm's `Lit::Float` case (deleting it would let
    // the inner `3.14` be recorded unsigned instead).
    let m = metrics_of("fn f(x: bool) { if x { let _ = -3.14; } }");
    assert!(
        m.magic_numbers.iter().any(|n| n.value == "-3.14"),
        "a negative float literal records its signed value, got {:?}",
        m.magic_numbers
    );
}

#[test]
fn non_negation_unary_on_int_records_unsigned_value() {
    // `!5` is a *non-negation* unary, so the negative-literal fast path must NOT
    // claim it — the inner `5` is recorded unsigned. Guards the
    // `matches!(op, UnOp::Neg(_))` match guard against firing on every unary.
    let m = metrics_of("fn f(x: bool) { if x { let _ = !5; } }");
    assert!(
        m.magic_numbers.iter().any(|n| n.value == "5"),
        "a `!` unary is not a negative literal; inner value stays unsigned, got {:?}",
        m.magic_numbers
    );
}

#[test]
fn const_context_resets_after_const_item() {
    // A `const`/`static` item suppresses magic numbers in its value, but the
    // suppression must be *popped* afterwards so a later literal is still
    // flagged. Guards the `in_const_context -= 1` restore in
    // `visit_item_const`/`visit_item_static`.
    let m = metrics_of("fn f(x: bool) { if x { const K: i32 = 42; let _ = 99; } }");
    assert!(
        m.magic_numbers.iter().any(|n| n.value == "99"),
        "the literal after a const item must still be a magic number"
    );
    assert!(
        !m.magic_numbers.iter().any(|n| n.value == "42"),
        "the const's own value is suppressed"
    );
}

#[test]
fn static_context_resets_after_static_item() {
    let m = metrics_of("fn f(x: bool) { if x { static S: i32 = 42; let _ = 99; } }");
    assert!(
        m.magic_numbers.iter().any(|n| n.value == "99"),
        "the literal after a static item must still be a magic number"
    );
}
