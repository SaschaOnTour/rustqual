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
