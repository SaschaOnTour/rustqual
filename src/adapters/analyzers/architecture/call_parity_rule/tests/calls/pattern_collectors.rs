//! Tests for the private pattern-ident collectors in `calls.rs`
//! (`extract_pat_ident_name`, `collect_pattern_idents` + its
//! `walk_each`/`push_pat_ident` helpers), the macro-body token parser
//! (`parse_macro_tokens`), and the `while`-loop visitor. Each isolates
//! one match arm / `-> ()` / `Stmt::Local` extraction.

use super::*;

/// Parse a pattern source into `syn::Pat` via a `let`-binding wrapper
/// (syn parses refutable patterns here without a refutability check, so
/// `Some(x)` / `A(x) | B(y)` work too). Nested `match` extraction keeps
/// this a 2-statement body — not the 3-`let` shape the DRY fragment
/// detector flags against `type_infer`'s pattern helper.
fn parse_pat(src: &str) -> syn::Pat {
    if let Ok(file) = syn::parse_str::<syn::File>(&format!("fn _t() {{ let {src} = _x; }}")) {
        if let syn::Item::Fn(f) = &file.items[0] {
            if let syn::Stmt::Local(local) = &f.block.stmts[0] {
                return local.pat.clone();
            }
        }
    }
    // Or-patterns (`A | B`) are only valid in a match arm. Nested `match`
    // extraction (not 3 `let`s) keeps this clear of the DRY fragment
    // detector vs `type_infer`'s pattern helper.
    let file: syn::File =
        syn::parse_str(&format!("fn _t() {{ match _x {{ {src} => () }} }}")).expect("parse pat");
    match &file.items[0] {
        syn::Item::Fn(f) => match &f.block.stmts[0] {
            syn::Stmt::Expr(syn::Expr::Match(m), _) => m.arms[0].pat.clone(),
            _ => unreachable!("match stmt"),
        },
        _ => unreachable!("fn item"),
    }
}

fn idents(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    collect_pattern_idents(&parse_pat(src), &mut out);
    out
}

// ── extract_pat_ident_name ──────────────────────────────────────────────

#[test]
fn extract_pat_ident_name_peels_ident_and_type() {
    // `x` → Some; `x: u8` peels the `Pat::Type` wrapper → Some; a tuple
    // binds no single ident → None. Pins `-> None`, the `Pat::Ident` arm,
    // and the `Pat::Type` arm.
    assert_eq!(
        extract_pat_ident_name(&parse_pat("x")).as_deref(),
        Some("x")
    );
    assert_eq!(
        extract_pat_ident_name(&parse_pat("x: u8")).as_deref(),
        Some("x"),
        "Pat::Type peeled to inner ident"
    );
    assert_eq!(extract_pat_ident_name(&parse_pat("(a, b)")), None);
}

// ── collect_pattern_idents (one arm per pattern kind) ───────────────────

#[test]
fn collect_pattern_idents_covers_every_binding_kind() {
    // Each arm contributes its bound idents. Pins the Ident/Type/Reference/
    // Paren/Tuple/TupleStruct/Struct/Slice/Or arms + `walk_each`/
    // `push_pat_ident` against deletion / `-> ()`.
    assert_eq!(idents("x"), vec!["x"], "Ident (push_pat_ident)");
    assert_eq!(idents("x: u8"), vec!["x"], "Type");
    assert_eq!(idents("&x"), vec!["x"], "Reference");
    assert_eq!(idents("(x)"), vec!["x"], "Paren");
    assert_eq!(idents("(a, b)"), vec!["a", "b"], "Tuple (walk_each)");
    assert_eq!(idents("Some(x)"), vec!["x"], "TupleStruct");
    assert_eq!(idents("Foo { a, b }"), vec!["a", "b"], "Struct");
    assert_eq!(idents("[a, b]"), vec!["a", "b"], "Slice");
    // Or takes the first case only.
    assert_eq!(idents("A(x) | B(y)"), vec!["x"], "Or (first case)");
}

#[test]
fn push_pat_ident_recurses_into_at_subpattern() {
    // `whole @ Some(inner)` binds both names — pins `push_pat_ident`'s
    // subpattern recursion against `-> ()`.
    assert_eq!(idents("whole @ Some(inner)"), vec!["whole", "inner"]);
}

// ── parse_macro_tokens ──────────────────────────────────────────────────

fn rendered(exprs: &[syn::Expr]) -> String {
    use quote::ToTokens;
    exprs
        .iter()
        .map(|e| e.to_token_stream().to_string())
        .collect::<Vec<_>>()
        .join(" | ")
}

#[test]
fn parse_macro_tokens_extracts_let_init_exprs() {
    // A `let x = foo();` statement in a macro body contributes its init
    // expression. Pins the `Stmt::Local(l)` arm against deletion (which
    // would drop the init through the `_ => None` fallback).
    let exprs = parse_macro_tokens(quote::quote! { let x = foo(); });
    assert!(
        !exprs.is_empty() && rendered(&exprs).contains("foo"),
        "let-init expr extracted: {}",
        rendered(&exprs)
    );
}

// ── visit_expr_while ────────────────────────────────────────────────────

#[test]
fn calls_inside_while_loop_are_collected() {
    // A call in a `while` body must be collected — pins `visit_expr_while`
    // against `-> ()` (which would skip the loop body entirely).
    let calls = calls_in(
        "pub fn f() { while cond() { helper_in_loop(); } }",
        "src/cli/h.rs",
        "f",
    );
    assert!(
        calls.iter().any(|c| c.contains("helper_in_loop")),
        "call in while body collected: {calls:?}"
    );
}
