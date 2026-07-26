//! The per-kind attribute and name look-ups.
//!
//! These are pure shape tables, and the risk they carry is a forgotten variant:
//! a kind that silently returns no attributes takes its scope-forming
//! `#[cfg(test)]` or `#[allow(dead_code)]` with it.

use crate::adapters::shared::item_shape::{
    expr_attrs, foreign_item_attrs, impl_item_attrs, item_attrs, item_ident, pat_attrs, stmt_attrs,
    trait_item_attrs,
};

fn items(code: &str) -> Vec<syn::Item> {
    syn::parse_file(code).expect("fixture must parse").items
}

#[test]
fn every_named_item_kind_yields_its_attributes_and_name() {
    let code = "#[a] fn f() {} #[a] struct S; #[a] enum E {} #[a] union U { x: u8 } \
                #[a] type T = u8; #[a] const C: u8 = 1; #[a] static X: u8 = 1; \
                #[a] trait Tr {} #[a] mod m {} #[a] use std::io;";
    for item in items(code) {
        assert_eq!(
            item_attrs(&item).len(),
            1,
            "every kind must expose its attributes"
        );
    }
    let named = items(code).into_iter().filter(|i| item_ident(i).is_some());
    assert_eq!(named.count(), 9, "only the `use` item declares no name");
}

#[test]
fn associated_and_foreign_item_kinds_yield_their_attributes() {
    let syn::Item::Impl(block) = &items(
        "impl S { #[a] const C: u8 = 1; #[a] fn f() {} \
                                         #[a] type T = u8; }",
    )[0] else {
        panic!("expected an impl block")
    };
    assert!(block.items.iter().all(|i| impl_item_attrs(i).len() == 1));

    let syn::Item::Trait(t) = &items("trait T { #[a] const C: u8; #[a] fn f(); #[a] type O; }")[0]
    else {
        panic!("expected a trait")
    };
    assert!(t.items.iter().all(|i| trait_item_attrs(i).len() == 1));

    let syn::Item::ForeignMod(m) = &items("extern \"C\" { #[a] fn f(); #[a] static S: u8; }")[0]
    else {
        panic!("expected an extern block")
    };
    assert!(m.items.iter().all(|i| foreign_item_attrs(i).len() == 1));
}

#[test]
fn statement_attributes_are_exposed_where_syn_offers_them() {
    // A `#[cfg(test)]` statement is absent from a non-test build, so it scopes
    // like an item. `Stmt::Item` yields nothing because the item dispatch
    // already scopes it; `Stmt::Expr` is the documented gap.
    let syn::Item::Fn(f) =
        &items("fn f() { #[a] let _ = 1; #[a] println!(\"x\"); #[a] struct S; #[a] 1 + 1; }")[0]
    else {
        panic!("expected a function")
    };
    let counts: Vec<usize> = f.block.stmts.iter().map(|s| stmt_attrs(s).len()).collect();
    assert_eq!(
        counts,
        vec![1, 1, 0, 1],
        "every statement shape but an item exposes its own attributes"
    );
}

#[test]
fn expression_attributes_are_reachable_for_every_shape_a_statement_takes() {
    // The match is exhaustive over a `#[non_exhaustive]` enum, so the risk is a
    // shape that silently falls into the catch-all and drops its scope. Asserted
    // through `stmt_attrs`, because that is the contract: for an assignment or a
    // binary expression syn binds the statement's attribute to the first
    // operand, and recovering it is part of the answer.
    let shapes = [
        "consume(x);",
        "x.consume();",
        "if x { }",
        "match x { _ => {} }",
        "for _ in x { }",
        "while x { }",
        "{ }",
        "unsafe { }",
        "return;",
        "x = 1;",
        "x += 1;",
        "x.field = 1;",
        "-x;",
        "x?;",
        "(x);",
    ];
    for shape in shapes {
        let code = format!("fn f() {{ #[a] {shape} }}");
        let syn::Item::Fn(f) = &items(&code)[0] else {
            panic!("expected a function")
        };
        assert_eq!(
            stmt_attrs(&f.block.stmts[0]).len(),
            1,
            "{shape}: the statement's attribute must be reachable"
        );
    }
}

#[test]
fn expr_attrs_reads_the_expression_itself() {
    // `stmt_attrs` recovers a statement attribute syn bound to an operand;
    // `expr_attrs` deliberately does not — it answers about the node it is
    // given, which is what every other caller needs.
    let syn::Item::Fn(f) = &items("fn f() { #[a] x = 1; }")[0] else {
        panic!("expected a function")
    };
    let syn::Stmt::Expr(expr, _) = &f.block.stmts[0] else {
        panic!("expected an expression statement")
    };
    assert_eq!(expr_attrs(expr).len(), 0, "the Assign node carries none");
    assert_eq!(stmt_attrs(&f.block.stmts[0]).len(), 1, "the statement does");
}

#[test]
fn pattern_attributes_are_reachable_for_every_shape() {
    // Same risk as the expression table: a variant that falls into the
    // catch-all silently drops its scope.
    let shapes = [
        "x", "Some(x)", "S { f }", "(a, b)", "[a, b]", "&x", "_", "1", "1..2",
    ];
    for shape in shapes {
        let code = format!("fn f(v: V) {{ match v {{ #[a] {shape} => {{}} _ => {{}} }} }}");
        let syn::Item::Fn(f) = &items(&code)[0] else {
            panic!("expected a function")
        };
        let syn::Stmt::Expr(syn::Expr::Match(m), _) = &f.block.stmts[0] else {
            panic!("expected a match")
        };
        // The arm carries the attribute; the pattern look-up must still reach
        // one placed directly on a pattern.
        assert_eq!(m.arms[0].attrs.len(), 1, "{shape}: arm attribute");
        assert!(
            pat_attrs(&m.arms[0].pat).is_empty(),
            "{shape}: the arm holds it, not the pattern"
        );
    }
    let code = "fn f(v: V) { let S { #[a] f: x } = v; }";
    let syn::Item::Fn(f) = &items(code)[0] else {
        panic!("expected a function")
    };
    let syn::Stmt::Local(l) = &f.block.stmts[0] else {
        panic!("expected a let")
    };
    let syn::Pat::Struct(p) = &l.pat else {
        panic!("expected a struct pattern")
    };
    assert_eq!(
        p.fields[0].attrs.len(),
        1,
        "a field pattern carries its own"
    );

    // And one placed on the pattern itself, which is where `pat_attrs` earns
    // its keep: a closure parameter is a bare pattern with no arm above it.
    let closure = "fn f() { let g = | #[a] x: u8 | x; }";
    let syn::Item::Fn(f) = &items(closure)[0] else {
        panic!("expected a function")
    };
    let syn::Stmt::Local(l) = &f.block.stmts[0] else {
        panic!("expected a let")
    };
    let Some(init) = &l.init else {
        panic!("expected an initialiser")
    };
    let syn::Expr::Closure(c) = &*init.expr else {
        panic!("expected a closure")
    };
    assert_eq!(
        pat_attrs(&c.inputs[0]).len(),
        1,
        "a closure parameter pattern carries its own attribute"
    );
}
