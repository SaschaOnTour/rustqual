//! Tests pinning the `Visit` walkers that populate `WorkspaceTypeIndex`
//! (alias / fn / method / trait collectors). Each isolates one survivor:
//! a `visit_* -> ()` (walk stops, item unrecorded), a `visit_item_mod ->
//! ()` (inline-module items unrecorded), the `||`→`&&` test-attr skip
//! (a `#[test]` item must NOT be indexed), and the alias generic-param
//! `GenericParam::Type` arm.

use super::*;

fn has_key_ending(index: &WorkspaceTypeIndex, suffix: &str) -> bool {
    index.type_aliases.keys().any(|k| k.ends_with(suffix))
        || index.fn_returns.keys().any(|k| k.ends_with(suffix))
        || index.trait_methods.keys().any(|k| k.ends_with(suffix))
}

// ── alias collector (collect_from_file / visit_item_type / visit_item_mod) ──

#[test]
fn type_alias_is_indexed_with_its_generic_params() {
    // A generic type alias must be recorded with its type-param names.
    // Pins `collect_from_file -> ()`, `visit_item_type -> ()`, and the
    // `GenericParam::Type(t)` arm (deleting it would drop `T`).
    let index = index_for(
        &[("src/app/a.rs", "pub type Wrapped<T> = Vec<T>;")],
        &["src/app/a.rs"],
    );
    let entry = index
        .type_aliases
        .iter()
        .find(|(k, _)| k.ends_with("::Wrapped"));
    let (_, def) = entry.expect("type alias `Wrapped` indexed");
    assert_eq!(def.params, vec!["T".to_string()], "generic param recorded");
}

#[test]
fn type_alias_inside_inline_mod_is_indexed() {
    // The alias collector must descend into inline modules. Pins
    // `AliasCollector::visit_item_mod -> ()` (which would skip the body).
    let index = index_for(
        &[(
            "src/app/a.rs",
            "pub mod inner { pub type Buried = String; }",
        )],
        &["src/app/a.rs"],
    );
    assert!(
        index
            .type_aliases
            .keys()
            .any(|k| k.contains("inner") && k.ends_with("::Buried")),
        "alias inside `mod inner` indexed: {:?}",
        index.type_aliases.keys().collect::<Vec<_>>()
    );
}

// ── fn collector (||→&& test-attr skip) ─────────────────────────────────

#[test]
fn test_attributed_free_fn_is_not_indexed() {
    // `has_cfg_test || has_test_attr` skips test fns from the return
    // index. Pins `||`→`&&` (which would require BOTH attrs, indexing the
    // `#[test]`-only fn). The plain fn is the positive control.
    let index = index_for(
        &[(
            "src/app/a.rs",
            "pub struct R;\n#[test]\nfn skipme() -> R { R }\npub fn keepme() -> R { R }",
        )],
        &["src/app/a.rs"],
    );
    assert!(
        index.fn_returns.keys().any(|k| k.ends_with("::keepme")),
        "plain fn indexed: {:?}",
        index.fn_returns.keys().collect::<Vec<_>>()
    );
    assert!(
        !index.fn_returns.keys().any(|k| k.ends_with("::skipme")),
        "#[test] fn must not be indexed: {:?}",
        index.fn_returns.keys().collect::<Vec<_>>()
    );
}

// ── method collector (||→&& test-attr skip + visit_item_mod) ────────────

#[test]
fn test_attributed_method_is_not_indexed() {
    // Same `||`→`&&` skip for impl methods.
    let index = index_for(
        &[(
            "src/app/a.rs",
            "pub struct S;\npub struct R;\nimpl S { #[test] fn skipm(&self) -> R { R } pub fn keepm(&self) -> R { R } }",
        )],
        &["src/app/a.rs"],
    );
    let recv = index
        .method_returns
        .keys()
        .find(|k| k.ends_with("::S"))
        .expect("receiver S indexed");
    let methods = &index.method_returns[recv];
    assert!(methods.contains_key("keepm"), "plain method indexed");
    assert!(
        !methods.contains_key("skipm"),
        "#[test] method must not be indexed: {methods:?}"
    );
}

#[test]
fn method_inside_inline_mod_is_indexed() {
    // The method collector must descend into inline modules. Pins
    // `MethodCollector::visit_item_mod -> ()`.
    let index = index_for(
        &[(
            "src/app/a.rs",
            "pub mod inner { pub struct S; pub struct R; impl S { pub fn m(&self) -> R { R } } }",
        )],
        &["src/app/a.rs"],
    );
    assert!(
        index
            .method_returns
            .keys()
            .any(|k| k.contains("inner") && k.ends_with("::S")),
        "method on type inside `mod inner` indexed: {:?}",
        index.method_returns.keys().collect::<Vec<_>>()
    );
}

// ── trait collector (visit_item_mod) ────────────────────────────────────

#[test]
fn trait_inside_inline_mod_is_indexed() {
    // The trait collector must descend into inline modules. Pins
    // `TraitCollector::visit_item_mod -> ()`.
    let index = index_for(
        &[(
            "src/app/a.rs",
            "pub mod inner { pub trait Svc { fn handle(&self); } }",
        )],
        &["src/app/a.rs"],
    );
    assert!(
        has_key_ending(&index, "::Svc")
            && index
                .trait_methods
                .keys()
                .any(|k| k.contains("inner") && k.ends_with("::Svc")),
        "trait inside `mod inner` indexed: {:?}",
        index.trait_methods.keys().collect::<Vec<_>>()
    );
}
