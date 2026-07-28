//! Per-construct token coverage for the structural normalizer (S2.6).
//!
//! The S2.2 mutation run left 69 survivors in `normalize.rs`: deleting any
//! `bin_op_str` / `un_op_str` arm, or any `visit_expr` / `visit_pat` match arm
//! that pushes a discriminator token, went undetected. These enumeration tests
//! pin the exact `NormalizedToken` each construct must emit, so a dropped arm
//! (which falls through to the catch-all / default walk and loses the token)
//! now fails a test.

use crate::adapters::shared::normalize::{normalize_fn, NormalizedToken};
use NormalizedToken::{BoolLit, CharLit, FloatLit, IntLit, Keyword, MacroCall, Operator, StrLit};

/// Normalize statement-level `code` into its token stream.
fn toks(code: &str) -> Vec<NormalizedToken> {
    let wrapped = format!("fn f() {{ {code} }}");
    let file = syn::parse_file(&wrapped).unwrap_or_else(|e| panic!("parse {code:?}: {e}"));
    let syn::Item::Fn(f) = &file.items[0] else {
        unreachable!("wrapped code is a fn")
    };
    normalize_fn(&f.sig, &f.block)
}

fn assert_emits(code: &str, want: &NormalizedToken) {
    let got = toks(code);
    assert!(
        got.contains(want),
        "{code:?} must emit {want:?}; got {got:?}"
    );
}

#[test]
fn binary_operators_each_emit_their_token() {
    let cases: [(&str, &str); 28] = [
        ("a + b;", "+"),
        ("a - b;", "-"),
        ("a * b;", "*"),
        ("a / b;", "/"),
        ("a % b;", "%"),
        ("a && b;", "&&"),
        ("a || b;", "||"),
        ("a ^ b;", "^"),
        ("a & b;", "&"),
        ("a | b;", "|"),
        ("a << b;", "<<"),
        ("a >> b;", ">>"),
        ("a == b;", "=="),
        ("a < b;", "<"),
        ("a <= b;", "<="),
        ("a != b;", "!="),
        ("a >= b;", ">="),
        ("a > b;", ">"),
        ("a += b;", "+="),
        ("a -= b;", "-="),
        ("a *= b;", "*="),
        ("a /= b;", "/="),
        ("a %= b;", "%="),
        ("a ^= b;", "^="),
        ("a &= b;", "&="),
        ("a |= b;", "|="),
        ("a <<= b;", "<<="),
        ("a >>= b;", ">>="),
    ];
    for (code, op) in cases {
        assert_emits(code, &Operator(op));
    }
}

#[test]
fn unary_operators_each_emit_their_token() {
    // Also kills the `un_op_str -> ""` / `"xyzzy"` return-replacement mutants.
    for (code, op) in [("-a;", "-"), ("!a;", "!"), ("*a;", "*")] {
        assert_emits(code, &Operator(op));
    }
}

#[test]
fn expr_kinds_each_emit_their_marker() {
    let cases: [(&str, NormalizedToken); 17] = [
        ("a = b;", Operator("=")),
        ("while a {}", Keyword("while")),
        ("loop {}", Keyword("loop")),
        ("break;", Keyword("break")),
        ("continue;", Keyword("continue")),
        ("a[b];", Operator("[]")),
        ("(a, b);", Keyword("tuple")),
        ("[a, b];", Keyword("array")),
        ("a.await;", Keyword("await")),
        ("a..b;", Operator("..")),
        ("a as u8;", Keyword("as")),
        ("[a; 3];", Keyword("array")),
        ("if let Some(a) = b {}", Keyword("let")),
        ("yield a;", Keyword("yield")),
        ("m!();", MacroCall("m".to_string())),
        // Expr-position macro (`Expr::Macro`, distinct from the `Stmt::Macro`
        // path above): the let-initialiser is the only way to reach the arm.
        ("let _ = m!();", MacroCall("m".to_string())),
        ("1.5;", FloatLit),
    ];
    for (code, want) in &cases {
        assert_emits(code, want);
    }
    // String + char literals in expression position.
    assert_emits("\"x\";", &StrLit);
    assert_emits("'c';", &CharLit);
}

#[test]
fn pat_kinds_each_emit_their_marker() {
    let cases: [(&str, NormalizedToken); 11] = [
        ("match v { (a, b) => {} }", Keyword("tuple")),
        ("match v { Foo(a) => {} }", Keyword("tuple")),
        ("match v { &a => {} }", Operator("&")),
        ("match v { [a, b] => {} }", Keyword("array")),
        ("match v { [a, ..] => {} }", Operator("..")),
        ("match v { 1..=5 => {} }", Operator("..")),
        ("match v { 1 => {} }", IntLit),
        ("match v { 1.5 => {} }", FloatLit),
        ("match v { \"x\" => {} }", StrLit),
        ("match v { true => {} }", BoolLit(true)),
        ("match v { 'c' => {} }", CharLit),
    ];
    for (code, want) in &cases {
        assert_emits(code, want);
    }
}

#[test]
fn or_pattern_emits_a_pipe_between_each_case() {
    // Kills the `Pat::Or` arm deletion (0 pipes) AND the `i > 0` comparison
    // mutants (`==` → 1, `<` → 0, `>=` → 3 pipes).
    let got = toks("match v { 1 | 2 | 3 => {} }");
    let pipes = got.iter().filter(|t| **t == Operator("|")).count();
    assert_eq!(
        pipes, 2,
        "`1 | 2 | 3` needs exactly 2 separating pipes; got {got:?}"
    );
}
