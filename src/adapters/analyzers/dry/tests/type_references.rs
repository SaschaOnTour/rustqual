//! What keeps a type alive.
//!
//! The model is deliberately coarse — every name occurrence counts, the same
//! grain the call graph uses (last path segment). Over-collecting only ever
//! suppresses a finding; under-collecting invents one, and telling someone to
//! delete a type that is in use is the expensive mistake.

use std::collections::HashSet;

use crate::adapters::analyzers::dry::collect_type_references;

/// `(production, test)` reference sets for one file at `path`, with `path`
/// itself optionally counted as a test file.
fn split_refs(path: &str, code: &str, test_files: &[&str]) -> (HashSet<String>, HashSet<String>) {
    let syntax = syn::parse_file(code).expect("fixture must parse");
    let parsed = vec![(path.to_string(), code.to_string(), syntax)];
    let cfg_test: HashSet<String> = test_files.iter().map(|f| f.to_string()).collect();
    collect_type_references(&parsed, &cfg_test)
}

fn refs(code: &str) -> HashSet<String> {
    split_refs("src/lib.rs", code, &[]).0
}

#[test]
fn a_name_in_a_field_type_counts() {
    assert!(refs("struct Holder { inner: Inner }").contains("Inner"));
}

#[test]
fn a_name_in_an_expression_path_counts() {
    assert!(refs("fn f() { let _ = Config::default(); }").contains("Config"));
}

#[test]
fn a_name_in_a_pattern_counts() {
    assert!(refs("fn f(v: u8) { match v { Kind::A => {} _ => {} } }").contains("Kind"));
}

#[test]
fn a_name_in_a_macro_body_counts() {
    // syn hands a macro body over as an opaque token stream, so an AST walk
    // alone sees nothing here — the single blind spot that would produce false
    // findings for macro-driven code.
    let set = refs("fn f() { let _ = vec![Widget::new()]; }");
    assert!(
        set.contains("Widget"),
        "macro token idents must count: {set:?}"
    );
}

#[test]
fn a_name_in_a_nested_macro_group_counts() {
    let set = refs("fn f() { html! { div { Component { x: 1 } } } }");
    assert!(set.contains("Component"), "nested groups too: {set:?}");
}

#[test]
fn a_declaration_is_not_a_use_of_itself() {
    // Otherwise nothing is ever dead.
    assert!(!refs("struct Lonely;").contains("Lonely"));
    assert!(!refs("enum Lonely { A }").contains("Lonely"));
    assert!(!refs("type Lonely = u8;").contains("Lonely"));
    assert!(!refs("const LONELY: u8 = 1;").contains("LONELY"));
    assert!(!refs("static LONELY: u8 = 1;").contains("LONELY"));
}

#[test]
fn an_impl_self_type_is_not_a_use() {
    // A type that only carries its own methods keeps nothing alive — the same
    // verdict rustc reaches with "never constructed".
    assert!(!refs("struct Lonely; impl Lonely { fn m(&self) {} }").contains("Lonely"));
    assert!(
        refs("struct Lonely; impl Default for Lonely { fn default() -> Self { Lonely } }")
            .contains("Lonely"),
        "only the self type is exempt — the impl's body still uses the name"
    );
}

#[test]
fn the_trait_of_a_trait_impl_is_a_use() {
    assert!(refs("struct S; impl Marker for S {}").contains("Marker"));
}

#[test]
fn generic_arguments_of_an_impl_self_type_are_uses() {
    // `impl Wrapper<Inner>` really does use `Inner`; only the type carrying the
    // methods is exempt.
    let set = refs("impl Wrapper<Inner> { fn m(&self) {} }");
    assert!(set.contains("Inner"), "generic argument is a use: {set:?}");
    assert!(!set.contains("Wrapper"));
}

#[test]
fn the_module_prefix_of_an_impl_self_type_is_a_use() {
    // Only the final segment names the type being implemented; skipping more
    // than that would drop real references.
    let set = refs("impl inner::Thing { fn m(&self) {} }");
    assert!(set.contains("inner"));
    assert!(!set.contains("Thing"));
}

#[test]
fn declarations_still_contribute_their_own_generics_and_attributes() {
    let set = refs("#[derive(Serialize)]\nstruct S<T: Bound> { t: T }");
    assert!(set.contains("Bound"), "generic bounds are uses: {set:?}");
    assert!(set.contains("Serialize"), "derive names are uses: {set:?}");
}

#[test]
fn test_and_production_references_are_kept_apart() {
    // A type used only from `#[cfg(test)]` code is test-only, which is a
    // different finding from "used nowhere".
    let code = "fn prod() { let _ = Used::new(); }\n\
                #[cfg(test)]\nmod tests { fn t() { let _ = OnlyInTests::new(); } }";
    let (production, tests) = split_refs("src/lib.rs", code, &[]);
    assert!(production.contains("Used") && !production.contains("OnlyInTests"));
    assert!(tests.contains("OnlyInTests"));
}

#[test]
fn a_whole_test_file_contributes_only_test_references() {
    let code = "fn helper() { let _ = Fixture::new(); }";
    let (production, tests) = split_refs("tests/it.rs", code, &["tests/it.rs"]);
    assert!(!production.contains("Fixture"));
    assert!(tests.contains("Fixture"));
}

#[test]
fn a_name_used_only_through_inline_format_args_counts() {
    // `proc_macro2` lexes a string as ONE literal, so a token walk never sees
    // `PREFIX` here. Inline format arguments have been idiomatic since Rust
    // 2021 — missing them would report constants that are plainly in use.
    let set = refs(r#"fn f() -> String { format!("{PREFIX}{path}") }"#);
    assert!(
        set.contains("PREFIX"),
        "interpolated name is a use: {set:?}"
    );
    assert!(set.contains("path"));
}

#[test]
fn names_in_a_format_spec_count_too() {
    // `{value:>width$}` names two arguments, not one.
    let set = refs(r#"fn f() { println!("{value:>width$}"); }"#);
    assert!(set.contains("value") && set.contains("width"), "{set:?}");
}

#[test]
fn a_positional_placeholder_contributes_nothing() {
    let set = refs(r#"fn f() { println!("{0} {:?}", x); }"#);
    assert!(!set.contains("0"));
}

#[test]
fn a_name_in_an_intra_doc_link_counts() {
    // `/// see [`MAX`]` is a real reference: deleting `MAX` breaks the link and
    // rustdoc warns about it. Only bracketed link targets count — harvesting
    // every word of prose would let any type whose name appears in a sentence
    // keep itself alive.
    let set = refs("/// See [`MAX`] and [Config] and [the docs](Parser).\npub fn f() {}");
    assert!(set.contains("MAX"), "code-span link target: {set:?}");
    assert!(set.contains("Config"), "plain link target: {set:?}");
    assert!(set.contains("Parser"), "link with text: {set:?}");
}

#[test]
fn prose_in_a_doc_comment_is_not_a_reference() {
    let set = refs("/// This parses a Config from the Parser.\npub fn f() {}");
    assert!(
        !set.contains("Config") && !set.contains("Parser"),
        "{set:?}"
    );
}

#[test]
fn a_name_used_only_in_a_doc_test_is_a_test_reference() {
    // A doc example is code `cargo test` compiles and runs, so the type it
    // names is in use — but it is *test* use, even though the documented item
    // is production code. Counting it as production would hide that nothing
    // else uses the type. Doc comments arrive as one `#[doc = "…"]` attribute
    // per line, so the fence has to be tracked across them.
    let code = "/// ```\n/// let x = krate::DocTested::new();\n/// ```\npub fn documented() {}";
    let (production, tests) = split_refs("src/lib.rs", code, &[]);
    assert!(
        tests.contains("DocTested"),
        "doc-test code is test use: {tests:?}"
    );
    assert!(!production.contains("DocTested"), "not production use");
}

#[test]
fn prose_outside_a_doc_fence_is_still_not_a_reference() {
    // The fence is what separates code from prose; without it, any type named
    // in a sentence would keep itself alive.
    let set = refs("/// Builds a DocTested from a Parser.\npub fn documented() {}");
    assert!(
        !set.contains("DocTested") && !set.contains("Parser"),
        "{set:?}"
    );
}

#[test]
fn a_use_path_counts_as_a_reference() {
    // Load-bearing and easy to remove by accident: it works only because syn's
    // default `visit_item_use` walk feeds the path idents to `visit_ident`. A
    // "don't count imports" cleanup would turn every re-exported facade type
    // into a finding.
    assert!(refs("use crate::inner::Facade;").contains("Facade"));
    assert!(refs("pub use crate::inner::Facade;").contains("Facade"));
}

#[test]
fn test_context_stays_sticky_through_nested_modules() {
    // A reference from a helper module nested inside `#[cfg(test)] mod` must
    // stay a test reference. If stickiness regressed, those references would
    // count as production and silently suppress every test-only finding.
    let code = "#[cfg(test)]\nmod tests { mod helpers { fn f() { let _ = Fixture; } } }";
    let (production, tests) = split_refs("src/lib.rs", code, &[]);
    assert!(
        !production.contains("Fixture"),
        "nested test helper is test code"
    );
    assert!(tests.contains("Fixture"));
}

#[test]
fn a_name_value_attribute_still_contributes_its_value() {
    // Overriding `visit_meta_name_value` for doc comments replaced syn's
    // default, which visited the value expression. Any other name-value
    // attribute would then contribute nothing — under-collection, which is the
    // direction that invents findings.
    assert!(refs("#[cfg_attr(feature = \"x\", derive(Trace))]\npub struct S;").contains("Trace"));
    let via_macro = refs("#[doc = include_str!(\"api.md\")]\npub fn f() {}");
    assert!(
        via_macro.contains("include_str"),
        "the value expression is walked even when it is not a literal: {via_macro:?}"
    );
}

#[test]
fn test_context_switches_on_every_item_kind() {
    // The switch has to sit at the item dispatch, not on the handful of shapes
    // that happened to need an override — a `#[cfg(test)]` const or struct
    // carries references just as a function does.
    let via_const = split_refs(
        "src/lib.rs",
        "pub struct Fixture;\n#[cfg(test)]\nconst CHECK: fn(Fixture) = |_| {};",
        &[],
    );
    assert!(!via_const.0.contains("Fixture"), "const: {:?}", via_const.0);
    assert!(via_const.1.contains("Fixture"));

    let via_struct = split_refs(
        "src/lib.rs",
        "pub struct Fixture;\n#[cfg(test)]\nstruct Holder(Fixture);",
        &[],
    );
    assert!(
        !via_struct.0.contains("Fixture"),
        "struct: {:?}",
        via_struct.0
    );

    let via_use = split_refs(
        "src/lib.rs",
        "#[cfg(test)]\nuse crate::inner::Fixture;",
        &[],
    );
    assert!(!via_use.0.contains("Fixture"), "use: {:?}", via_use.0);
}
