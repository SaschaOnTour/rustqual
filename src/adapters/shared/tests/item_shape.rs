//! The per-kind attribute and name look-ups.
//!
//! These are pure shape tables, and the risk they carry is a forgotten variant:
//! a kind that silently returns no attributes takes its scope-forming
//! `#[cfg(test)]` or `#[allow(dead_code)]` with it.

use crate::adapters::shared::item_shape::{
    foreign_item_attrs, impl_item_attrs, item_attrs, item_ident, trait_item_attrs,
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
