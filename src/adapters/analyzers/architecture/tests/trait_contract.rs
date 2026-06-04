//! Unit tests for the Trait-Signature rule.
//!
//! Each test builds a `CompiledTraitContract` directly, parses a fixture
//! file, and asserts the expected set of violations. The fixture files
//! are synthesized inline — no external golden examples here; the
//! golden-example suite covers the `forbid_*` matchers.

use crate::adapters::analyzers::architecture::trait_contract_rule::{
    check_trait_contracts, CompiledTraitContract,
};
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use globset::{Glob, GlobSet, GlobSetBuilder};

fn parse(src: &str) -> syn::File {
    syn::parse_str(src).expect("test fixture must parse")
}

fn globset(patterns: &[&str]) -> GlobSet {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).expect("valid glob"));
    }
    b.build().expect("valid glob set")
}

fn empty() -> CompiledTraitContract {
    CompiledTraitContract {
        name: "t".into(),
        scope: globset(&["**/*.rs"]),
        receiver_may_be: None,
        required_param_type_contains: None,
        forbidden_return_type_contains: Vec::new(),
        forbidden_error_variant_contains: Vec::new(),
        error_types: Vec::new(),
        methods_must_be_async: None,
        must_be_object_safe: None,
        required_supertraits_contain: Vec::new(),
    }
}

fn run(file: &str, src: &str, rule: &CompiledTraitContract) -> Vec<MatchLocation> {
    let ast = parse(src);
    check_trait_contracts(&[(file.to_string(), &ast)], std::slice::from_ref(rule))
}

fn checks(hits: &[MatchLocation]) -> Vec<&'static str> {
    hits.iter()
        .filter_map(|h| match &h.kind {
            ViolationKind::TraitContract { check, .. } => Some(*check),
            _ => None,
        })
        .collect()
}

// ── scope ─────────────────────────────────────────────────────────────

#[test]
fn out_of_scope_file_is_skipped() {
    let mut rule = empty();
    rule.scope = globset(&["src/ports/**"]);
    rule.methods_must_be_async = Some(true);
    let src = "pub trait Svc { fn f(&self); }";
    let hits = run("src/other/x.rs", src, &rule);
    assert!(hits.is_empty());
}

#[test]
fn non_trait_items_are_ignored() {
    let mut rule = empty();
    rule.methods_must_be_async = Some(true);
    let src = r#"
        pub fn plain() {}
        pub struct S;
        impl S { pub fn f(&self) {} }
    "#;
    assert!(run("any.rs", src, &rule).is_empty());
}

// ── receiver_may_be ───────────────────────────────────────────────────

#[test]
fn receiver_shared_ref_only_flags_mut_receivers() {
    let mut rule = empty();
    rule.receiver_may_be = Some(vec!["shared_ref".into()]);
    let src = r#"
        pub trait Svc {
            fn read(&self);
            fn write(&mut self);
            fn consume(self);
        }
    "#;
    let hits = run("any.rs", src, &rule);
    assert_eq!(checks(&hits), vec!["receiver", "receiver"]);
}

#[test]
fn receiver_any_accepts_all_forms() {
    let mut rule = empty();
    rule.receiver_may_be = Some(vec!["any".into()]);
    let src = r#"
        pub trait Svc {
            fn read(&self);
            fn write(&mut self);
            fn consume(self);
        }
    "#;
    assert!(run("any.rs", src, &rule).is_empty());
}

#[test]
fn receiver_associated_fn_without_receiver_not_flagged() {
    let mut rule = empty();
    rule.receiver_may_be = Some(vec!["shared_ref".into()]);
    let src = "pub trait Build { fn make() -> Self where Self: Sized; }";
    assert!(run("any.rs", src, &rule).is_empty());
}

// ── methods_must_be_async ─────────────────────────────────────────────

#[test]
fn methods_must_be_async_flags_sync_methods() {
    let mut rule = empty();
    rule.methods_must_be_async = Some(true);
    let src = r#"
        pub trait Svc {
            async fn a(&self);
            fn b(&self);
        }
    "#;
    let hits = run("any.rs", src, &rule);
    assert_eq!(checks(&hits), vec!["async"]);
}

// ── forbidden_return_type_contains ────────────────────────────────────

#[test]
fn forbidden_return_type_matches_substring() {
    let mut rule = empty();
    rule.forbidden_return_type_contains = vec!["anyhow::".into(), "Box<dyn".into()];
    let src = r#"
        pub trait Svc {
            fn a(&self) -> anyhow::Result<()>;
            fn b(&self) -> Result<Box<dyn std::error::Error>, ()>;
            fn c(&self) -> Result<(), String>;
        }
    "#;
    let hits = run("any.rs", src, &rule);
    assert_eq!(checks(&hits), vec!["return_type", "return_type"]);
}

// ── required_param_type_contains ──────────────────────────────────────

#[test]
fn required_param_fires_when_none_of_the_params_match() {
    let mut rule = empty();
    rule.required_param_type_contains = Some("CancellationToken".into());
    let src = r#"
        pub trait Svc {
            fn with_ctx(&self, ctx: CancellationToken);
            fn without(&self, path: String);
        }
    "#;
    let hits = run("any.rs", src, &rule);
    assert_eq!(checks(&hits), vec!["required_param"]);
}

#[test]
fn required_param_flags_the_method_that_lacks_the_param_not_the_one_that_has_it() {
    // The `!has_required` guard means the fn WITHOUT the param is flagged.
    // Asserting on count alone can't catch a deleted `!` (it would flag the
    // other method, keeping the count at 1) — pin the flagged method's
    // identity via the detail message.
    let mut rule = empty();
    rule.required_param_type_contains = Some("CancellationToken".into());
    let src = r#"
        pub trait Svc {
            fn with_ctx(&self, ctx: CancellationToken);
            fn without(&self, path: String);
        }
    "#;
    let hits = run("any.rs", src, &rule);
    let details: Vec<&str> = hits
        .iter()
        .filter_map(|h| match &h.kind {
            ViolationKind::TraitContract {
                check: "required_param",
                detail,
                ..
            } => Some(detail.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(details.len(), 1, "exactly one required_param hit: {hits:?}");
    assert!(
        details[0].contains("without"),
        "the method LACKING the param is flagged: {details:?}"
    );
    assert!(
        !details[0].contains("with_ctx"),
        "the method that HAS the param is not flagged: {details:?}"
    );
}

// ── required_supertraits_contain ──────────────────────────────────────

#[test]
fn required_supertraits_flags_missing_bound() {
    let mut rule = empty();
    rule.required_supertraits_contain = vec!["Send".into(), "Sync".into()];
    let src = r#"
        pub trait A: Send + Sync {}
        pub trait B: Send {}
        pub trait C {}
    "#;
    let hits = run("any.rs", src, &rule);
    // B is missing Sync (1 hit); C is missing both Send and Sync (2 hits) = 3 total.
    assert_eq!(
        checks(&hits),
        vec!["supertrait", "supertrait", "supertrait"]
    );
}

// ── must_be_object_safe ───────────────────────────────────────────────

#[test]
fn must_be_object_safe_flags_self_return() {
    let mut rule = empty();
    rule.must_be_object_safe = Some(true);
    let src = r#"
        pub trait A { fn clone_box(&self) -> Self; }
        pub trait B { fn do_it(&self) -> (); }
    "#;
    let hits = run("any.rs", src, &rule);
    assert_eq!(checks(&hits), vec!["object_safety"]);
}

#[test]
fn must_be_object_safe_flags_generic_method() {
    let mut rule = empty();
    rule.must_be_object_safe = Some(true);
    let src = r#"
        pub trait A { fn cast<T>(&self, x: T); }
    "#;
    let hits = run("any.rs", src, &rule);
    assert_eq!(checks(&hits), vec!["object_safety"]);
}

#[test]
fn must_be_object_safe_allows_method_level_lifetime() {
    // Rust's actual dyn-compatibility rule treats lifetime params on
    // methods as dyn-safe — only type and const generics break
    // object-safety (the compiler can't synthesise a vtable entry for
    // unknown `T` or `const N`, but lifetimes are compile-time-only
    // and erased). Streaming traits routinely tie a returned `Box<dyn
    // Iterator + 'a>` to `&'a self` via a lifetime param. The check
    // must not false-flag this idiom.
    let mut rule = empty();
    rule.must_be_object_safe = Some(true);
    let src = r#"
        pub trait Streamable {
            fn stream<'a>(&'a self) -> Box<dyn Iterator<Item = u8> + 'a>;
        }
    "#;
    let hits = run("any.rs", src, &rule);
    assert!(
        hits.is_empty(),
        "method-level lifetime params are object-safe; expected no \
         findings but got {hits:?}",
    );
}

#[test]
fn must_be_object_safe_flags_const_generic_method() {
    // Defensive: const-generic method params break object-safety
    // (same vtable-synthesis problem as type generics). Lock in the
    // current behaviour so the lifetime-fix doesn't accidentally
    // widen the exemption.
    let mut rule = empty();
    rule.must_be_object_safe = Some(true);
    let src = r#"
        pub trait A { fn pack<const N: usize>(&self, data: [u8; N]); }
    "#;
    let hits = run("any.rs", src, &rule);
    assert_eq!(checks(&hits), vec!["object_safety"]);
}

// ── forbidden_error_variant_contains ──────────────────────────────────

#[test]
fn error_variant_substring_flagged_via_naming() {
    // File-local error type matched by naming convention (ends in `Error`).
    let mut rule = empty();
    rule.forbidden_error_variant_contains = vec!["syn::".into()];
    let src = r#"
        pub enum MyError {
            Parse(syn::Error),
            Other(String),
        }
        pub trait Svc { fn f(&self) -> Result<(), MyError>; }
    "#;
    let hits = run("any.rs", src, &rule);
    assert_eq!(checks(&hits), vec!["error_variant"]);
}

#[test]
fn error_variant_check_inspects_the_named_enum_not_the_first_one() {
    // `find_enum_in_file` must match the error enum BY NAME. A decoy enum
    // declared first carries the forbidden `syn::` field; the method's
    // actual error type (`MyError`, declared second) is clean. Pins the
    // `e.ident == name` match guard against `true` (which would pick the
    // first enum and wrongly flag it).
    let mut rule = empty();
    rule.forbidden_error_variant_contains = vec!["syn::".into()];
    let src = r#"
        pub enum DecoyError { Bad(syn::Error) }
        pub enum MyError { Clean(String) }
        pub trait Svc { fn f(&self) -> Result<(), MyError>; }
    "#;
    let hits = run("any.rs", src, &rule);
    assert!(
        checks(&hits).is_empty(),
        "must inspect MyError (clean), not the first-declared DecoyError: {hits:?}"
    );
}

// ── combined: clean trait passes all checks ───────────────────────────

#[test]
fn fully_compliant_trait_has_no_hits() {
    let mut rule = empty();
    rule.receiver_may_be = Some(vec!["shared_ref".into()]);
    rule.methods_must_be_async = Some(true);
    rule.forbidden_return_type_contains = vec!["anyhow::".into()];
    rule.required_supertraits_contain = vec!["Send".into(), "Sync".into()];
    rule.must_be_object_safe = Some(true);
    let src = r#"
        pub trait Svc: Send + Sync {
            async fn read(&self) -> Result<String, MyError>;
        }
    "#;
    assert!(run("any.rs", src, &rule).is_empty());
}

// ── trait inside inline module is still checked ──────────────────────

#[test]
fn trait_inside_inline_module_is_checked() {
    let mut rule = empty();
    rule.forbidden_return_type_contains = vec!["anyhow::".into()];
    let src = r#"
        mod inner {
            pub trait Svc {
                fn read(&self) -> anyhow::Result<()>;
            }
        }
    "#;
    let hits = run("any.rs", src, &rule);
    assert_eq!(
        checks(&hits),
        vec!["return_type"],
        "trait defined inside `mod inner {{ ... }}` must be checked"
    );
}

#[test]
fn trait_in_nested_inline_module_is_checked() {
    let mut rule = empty();
    rule.forbidden_return_type_contains = vec!["anyhow::".into()];
    let src = r#"
        mod outer {
            pub mod middle {
                pub mod inner {
                    pub trait Svc {
                        fn read(&self) -> anyhow::Result<()>;
                    }
                }
            }
        }
    "#;
    let hits = run("any.rs", src, &rule);
    assert_eq!(
        checks(&hits),
        vec!["return_type"],
        "traits inside deeply nested inline modules must be checked"
    );
}

#[test]
fn trait_contract_hit_to_finding_projects_rule_id_and_fields() {
    use crate::adapters::analyzers::architecture::trait_contract_rule::hit_to_finding;
    use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
    let hit = MatchLocation {
        file: "src/ports/x.rs".into(),
        line: 12,
        column: 1,
        kind: ViolationKind::TraitContract {
            trait_name: "Repo".into(),
            check: "receiver",
            detail: "must take &self".into(),
        },
    };
    let f = hit_to_finding(hit);
    assert_eq!(f.file, "src/ports/x.rs");
    assert_eq!(f.line, 12);
    assert_eq!(f.column, 1);
    assert_eq!(f.dimension, crate::domain::Dimension::Architecture);
    assert_eq!(
        f.rule_id, "architecture/trait_contract/receiver",
        "rule id embeds the check name (pins the TraitContract arm)"
    );
    assert_eq!(f.severity, crate::domain::Severity::High);
}

#[test]
fn render_type_collapses_path_colons() {
    use crate::adapters::analyzers::architecture::trait_contract_rule::rendering::render_type;
    let ty: syn::Type = syn::parse_str("std::fmt::Debug").unwrap();
    assert_eq!(
        render_type(&ty),
        "std::fmt::Debug",
        "`::` closed up, no stray spaces"
    );
    let generic: syn::Type = syn::parse_str("Vec<String>").unwrap();
    assert_eq!(render_type(&generic), "Vec<String>");
}
