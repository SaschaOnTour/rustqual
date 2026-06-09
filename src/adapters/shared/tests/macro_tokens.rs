//! Tests for the shared macro-token recovery helpers — the single place that
//! re-derives calls/idents/`self` from a macro's opaque token stream.

use crate::adapters::shared::macro_tokens::{
    idents_in_call_position, recover_exprs, tokens_reference_ident,
};
use proc_macro2::TokenStream;
use syn::visit::Visit;

/// Parse a fragment of macro-body tokens (the stuff between the delimiters).
fn ts(src: &str) -> TokenStream {
    src.parse().expect("token stream must lex")
}

/// Collect free-call callee idents (`foo()` → "foo") from recovered exprs, so a
/// test can assert a macro-embedded call survived recovery.
fn call_idents(exprs: &[syn::Expr]) -> Vec<String> {
    #[derive(Default)]
    struct V(Vec<String>);
    impl<'ast> Visit<'ast> for V {
        fn visit_expr_call(&mut self, n: &'ast syn::ExprCall) {
            if let syn::Expr::Path(p) = &*n.func {
                if let Some(seg) = p.path.segments.last() {
                    self.0.push(seg.ident.to_string());
                }
            }
            syn::visit::visit_expr_call(self, n);
        }
    }
    let mut v = V::default();
    exprs.iter().for_each(|e| v.visit_expr(e));
    v.0
}

// ── recover_exprs: the three escalating strategies ─────────────────────

#[test]
fn recover_exprs_comma_list_form() {
    // `assert_eq!(a(), b())` body — the common comma-separated expr list.
    let calls = call_idents(&recover_exprs(&ts("a(), b()")));
    assert!(
        calls.contains(&"a".to_string()) && calls.contains(&"b".to_string()),
        "comma form must recover both calls: {calls:?}"
    );
}

#[test]
fn recover_exprs_repeat_semicolon_form() {
    // `vec![helper(); 3]` body — the historical blind spot: the `;` separator
    // makes the comma-list parse fail; the braced-block fallback recovers it.
    let calls = call_idents(&recover_exprs(&ts("helper(); 3")));
    assert!(
        calls.contains(&"helper".to_string()),
        "repeat (`;`) form must recover the call via the block fallback: {calls:?}"
    );
}

#[test]
fn recover_exprs_repeat_form_recovers_nested_macro_stmt() {
    // `vec![format!("inner"); 3]` body — the block fallback turns
    // `format!("inner");` into a `Stmt::Macro`; it must be lifted back to an
    // `Expr::Macro` so a nested macro call is not dropped.
    let exprs = recover_exprs(&ts(r#"format!("inner"); 3"#));
    let has_format = exprs
        .iter()
        .any(|e| matches!(e, syn::Expr::Macro(m) if m.mac.path.is_ident("format")));
    assert!(has_format, "nested format! in a `;`-body must be recovered");
}

#[test]
fn recover_exprs_single_expr_form() {
    let calls = call_idents(&recover_exprs(&ts("only_call()")));
    assert!(
        calls.contains(&"only_call".to_string()),
        "single-expr form must recover the call: {calls:?}"
    );
}

#[test]
fn recover_exprs_unparseable_dsl_returns_empty_without_panic() {
    // A custom-DSL body that parses as none of expr-list / block / expr must
    // yield an empty Vec rather than panic — reachability falls back to
    // `idents_in_call_position`. `=> =>` is not valid in any of the three grammars.
    let exprs = recover_exprs(&ts("=> => =>"));
    assert!(
        exprs.is_empty(),
        "unparseable DSL must recover nothing, got {} exprs",
        exprs.len()
    );
}

// ── tokens_reference_ident: the SLM yes/no primitive ───────────────────

#[test]
fn tokens_reference_ident_finds_self_in_format_args() {
    // The SLM bug: `format!("[{}]", self.name)` references self in macro args.
    assert!(
        tokens_reference_ident(&ts(r#""[{}]", self.name"#), "self"),
        "self inside format! args must be found"
    );
}

#[test]
fn tokens_reference_ident_recurses_into_nested_groups() {
    assert!(
        tokens_reference_ident(&ts("outer(inner(self))"), "self"),
        "self nested in groups must be found"
    );
}

#[test]
fn tokens_reference_ident_false_when_absent() {
    assert!(
        !tokens_reference_ident(&ts(r#""[{}]", other.name"#), "self"),
        "must not find self when it is not referenced"
    );
}

#[test]
fn tokens_reference_ident_ignores_string_literal_and_substring() {
    // A `self` only inside a string literal is not an ident (proc_macro2 lexes
    // it as one Literal), and `local_self` must not substring-match `self` —
    // both are false, guarding SLM against this class of false positive.
    assert!(
        !tokens_reference_ident(&ts(r#""self is great""#), "self"),
        "self only inside a string literal is not a reference"
    );
    assert!(
        !tokens_reference_ident(&ts("local_self.field"), "self"),
        "must not substring-match `self` inside `local_self`"
    );
}

// ── idents_in_call_position: the precise reachability fallback ──────────

#[test]
fn idents_in_call_position_harvests_calls_and_component_renders() {
    // `Component { child: Leaf(x) }`-style DSL: the component (UpperCamelCase
    // ident + `{`) and the nested call `Leaf(x)` (ident + `(`) must surface...
    let ids: Vec<String> = idents_in_call_position(&ts("Component { child: Leaf(x) }")).collect();
    for name in ["Component", "Leaf"] {
        assert!(
            ids.iter().any(|i| i == name),
            "call/construction-position ident `{name}` must be collected, got {ids:?}"
        );
    }
    // ...but a prop key (`child:`) and a bare argument (`x`) must NOT — this is
    // the false-negative gap fix: a prop named like a production fn must not be
    // recorded as a call.
    for name in ["child", "x"] {
        assert!(
            !ids.iter().any(|i| i == name),
            "non-call ident `{name}` must be excluded, got {ids:?}"
        );
    }
}

#[test]
fn idents_in_call_position_excludes_lowercase_dsl_element_tag() {
    // `div { class: "log", Widget {} }` — `div` is a lowercase DSL element tag,
    // not a component/struct, so it must NOT be harvested (else an unused
    // `fn div()` would be masked by a UI tag). The UpperCamelCase `Widget`
    // render is still captured.
    let ids: Vec<String> =
        idents_in_call_position(&ts(r#"div { class: "log", Widget {} }"#)).collect();
    assert!(
        !ids.iter().any(|i| i == "div"),
        "lowercase element tag `div` must NOT be harvested, got {ids:?}"
    );
    assert!(
        ids.iter().any(|i| i == "Widget"),
        "UpperCamelCase component `Widget` must still be harvested, got {ids:?}"
    );
}

#[test]
fn idents_in_call_position_keeps_lowercase_call_form() {
    // A lowercase *call* `helper()` (paren group) is still harvested regardless
    // of case — only the brace-group construction form is case-gated.
    let ids: Vec<String> = idents_in_call_position(&ts("div { onclick: helper() }")).collect();
    assert!(
        ids.iter().any(|i| i == "helper"),
        "lowercase call `helper()` must still be harvested, got {ids:?}"
    );
}

#[test]
fn idents_in_call_position_excludes_prop_named_like_a_function() {
    // The exact Codex example: `Widget { helper: "x" }` — `helper` is a prop
    // key, not a call. It must not be harvested, or a `fn helper` would be
    // wrongly marked used/tested.
    let ids: Vec<String> = idents_in_call_position(&ts(r#"Widget { helper: "x" }"#)).collect();
    assert!(ids.iter().any(|i| i == "Widget"), "Widget render harvested");
    assert!(
        !ids.iter().any(|i| i == "helper"),
        "prop key `helper` must NOT be harvested, got {ids:?}"
    );
}
