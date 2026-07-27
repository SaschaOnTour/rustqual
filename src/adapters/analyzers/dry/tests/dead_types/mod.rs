//! DRY-006: declarations nothing refers to.
//!
//! This file holds the shared fixture helpers and the base cases; the two
//! halves that grew their own shape live next to it — `reachability` (what
//! keeps a declaration alive, including cycles) and `exemptions` (the
//! attributes and markers that excuse one).

mod exemptions;
mod reachability;

use std::collections::{HashMap, HashSet};

use crate::adapters::analyzers::dry::dead_types::*;
use crate::adapters::shared::declared_type::TypeItemKind;

type Markers = HashMap<String, HashSet<usize>>;

fn detect(code: &str) -> Vec<DeadTypeWarning> {
    detect_with(code, &Markers::new(), &Markers::new(), &HashSet::new())
}

fn detect_with(
    code: &str,
    api_lines: &Markers,
    test_helper_lines: &Markers,
    cfg_test_files: &HashSet<String>,
) -> Vec<DeadTypeWarning> {
    let syntax = syn::parse_file(code).expect("fixture must parse");
    let parsed = vec![("src/lib.rs".to_string(), code.to_string(), syntax)];
    detect_dead_types(&parsed, api_lines, test_helper_lines, cfg_test_files)
}

fn names(code: &str) -> Vec<String> {
    detect(code).into_iter().map(|w| w.name).collect()
}

fn markers(lines: &[usize]) -> Markers {
    let mut m = Markers::new();
    m.insert("src/lib.rs".to_string(), lines.iter().copied().collect());
    m
}

#[test]
fn a_declaration_nothing_refers_to_is_reported() {
    let found = detect("struct Orphan { field: u8 }");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "Orphan");
    assert_eq!(found[0].item, TypeItemKind::Struct);
    assert_eq!(found[0].kind, DeadTypeKind::Unused);
    assert_eq!(found[0].line, 1);
}

#[test]
fn a_referenced_declaration_is_not_reported() {
    assert!(names("struct Used; fn f(u: Used) {}").is_empty());
    assert!(names("const MAX: u8 = 1; fn f() -> u8 { MAX }").is_empty());
    assert!(names("type Alias = u8; fn f(a: Alias) {}").is_empty());
}

#[test]
fn a_declaration_used_only_from_a_macro_body_is_not_reported() {
    // The blind spot that would make macro-driven code look dead.
    assert!(names("struct Widget; fn f() { let _ = vec![Widget]; }").is_empty());
}

#[test]
fn a_declaration_used_only_from_tests_is_reported_as_test_only() {
    let found = detect(
        "pub struct Fixture;\n#[cfg(test)]\nmod tests { use super::Fixture; \
         fn t() { let _ = Fixture; } }",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].kind, DeadTypeKind::TestOnly);
}

#[test]
fn a_declaration_in_test_code_is_not_reported() {
    // Test-only declarations are test code by right — the same exemption
    // DRY-002 gives test functions.
    assert!(names("#[cfg(test)]\nmod tests { struct Fixture; }").is_empty());
}

#[test]
fn every_kind_carries_its_own_word_in_the_suggestion() {
    // The message has to name what it found — "const `MAX` is never used"
    // reads very differently from "struct `Foo` is never used".
    let found = detect("struct S; const C: u8 = 1;");
    let words: HashSet<&str> = found.iter().map(|w| w.item.label()).collect();
    assert_eq!(words, ["struct", "const"].into_iter().collect());
    let struct_finding = found.iter().find(|w| w.name == "S").expect("S reported");
    assert!(struct_finding.suggestion.contains("struct `S`"));
}

#[test]
fn a_constant_used_only_through_an_inline_format_arg_is_not_reported() {
    // The case that showed up on rustqual's own source: a const referenced
    // only as `format!("{PREFIX}…")`.
    let code = "const PREFIX: &str = \"x\";\nfn f(p: u8) -> String { format!(\"{PREFIX}{p}\") }";
    assert!(names(code).is_empty(), "{:?}", detect(code));
}

#[test]
fn a_declaration_in_a_test_file_is_not_reported() {
    // The whole-file exemption, which the `#[cfg(test)] mod` case does not
    // exercise — it runs through a different flag. Without it, every fixture
    // struct in every `tests/**.rs` of every project would be a finding.
    let code = "pub struct Fixture; fn build() -> Fixture { Fixture }";
    let syntax = syn::parse_file(code).expect("fixture must parse");
    let parsed = vec![("tests/it.rs".to_string(), code.to_string(), syntax)];
    let cfg_test: HashSet<String> = ["tests/it.rs".to_string()].into_iter().collect();
    let found = detect_dead_types(&parsed, &Markers::new(), &Markers::new(), &cfg_test);
    assert!(
        found.is_empty(),
        "test-file declarations are test code: {found:?}"
    );
}

#[test]
fn a_production_type_used_only_from_a_test_file_is_test_only() {
    // The counterpart: the reference set splits by file too, not just by
    // `#[cfg(test)]` blocks.
    let lib = "pub struct Fixture;";
    let test = "fn build() -> Fixture { Fixture }";
    let parsed = vec![
        (
            "src/lib.rs".to_string(),
            lib.to_string(),
            syn::parse_file(lib).expect("parse"),
        ),
        (
            "tests/it.rs".to_string(),
            test.to_string(),
            syn::parse_file(test).expect("parse"),
        ),
    ];
    let cfg_test: HashSet<String> = ["tests/it.rs".to_string()].into_iter().collect();
    let found = detect_dead_types(&parsed, &Markers::new(), &Markers::new(), &cfg_test);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].kind, DeadTypeKind::TestOnly);
}

#[test]
fn every_declaration_kind_reaches_a_finding() {
    // `declared_types` proves all six kinds are collected; each also has its own
    // self-name-skip in the reference collector, so each needs to be seen
    // through the whole detector at least once.
    let found = detect(
        "struct S; enum E {} union U { a: u8 } type A = u8; \
         const C: u8 = 1; static T: u8 = 1;",
    );
    let kinds: HashSet<TypeItemKind> = found.iter().map(|w| w.item).collect();
    assert_eq!(kinds.len(), 6, "every kind must be reportable: {found:?}");
}

#[test]
fn a_type_used_only_from_a_cfg_test_function_is_test_only() {
    // The context switch has to happen on every attributed item, not just on
    // modules and impl blocks — otherwise the reference counts as production
    // use and no finding is produced at all.
    let found = detect("pub struct Fixture;\n#[cfg(test)]\nfn helper() { let _ = Fixture; }");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].kind, DeadTypeKind::TestOnly);
}

#[test]
fn a_type_used_only_from_a_doc_example_is_test_only() {
    // A doc example is code `cargo test` runs, so it is test code — the same
    // treatment `tests/**` gets. The remedy the message names is `qual:api`.
    let found =
        detect("pub struct Example;\n/// ```\n/// let _ = Example;\n/// ```\npub fn docs() {}");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].kind, DeadTypeKind::TestOnly);
}
