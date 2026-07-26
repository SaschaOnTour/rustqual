//! DRY-006: declarations nothing refers to.

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
fn allow_dead_code_excludes_the_declaration() {
    // The author already told the compiler this is intentional.
    assert!(names("#[allow(dead_code)]\nstruct Kept;").is_empty());
}

#[test]
fn qual_api_excludes_the_declaration() {
    // The whole point of the marker: consumers live outside the analysed code,
    // so having no in-workspace user is expected. Before DRY-006 this marker
    // did nothing on a type and was reported as inert.
    let found = detect_with(
        "// qual:api\npub struct Entry;",
        &markers(&[1]),
        &Markers::new(),
        &HashSet::new(),
    );
    assert!(found.is_empty(), "qual:api must exclude a type: {found:?}");
}

#[test]
fn qual_test_helper_silences_test_only_but_not_unused() {
    // The marker's narrow purpose, mirrored from DRY-002: it excuses "only
    // tests use it", never "nothing uses it".
    let used_by_tests = "// qual:test_helper\npub struct Fixture;\n#[cfg(test)]\n\
                         mod tests { use super::Fixture; fn t() { let _ = Fixture; } }";
    assert!(
        detect_with(
            used_by_tests,
            &Markers::new(),
            &markers(&[1]),
            &HashSet::new()
        )
        .is_empty(),
        "test_helper excuses the test-only finding"
    );
    let used_by_nobody = "// qual:test_helper\npub struct Fixture;";
    assert_eq!(
        detect_with(
            used_by_nobody,
            &Markers::new(),
            &markers(&[1]),
            &HashSet::new()
        )
        .len(),
        1,
        "a helper nothing refers to is still dead"
    );
}

#[test]
fn a_type_that_only_carries_its_own_methods_is_reported() {
    // rustc reaches the same verdict ("never constructed"); a trait impl does
    // not keep a type alive either.
    let found = detect("struct Lonely; impl Lonely { fn m(&self) {} }");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].name, "Lonely");
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
fn an_underscore_prefixed_declaration_is_not_reported() {
    // `_name` is the language's own "deliberately unused" convention, and
    // rustc's dead_code lint honours it. Contradicting that would make the
    // check argue with the compiler.
    assert!(names("const _KEPT_FOR_DOCS: u8 = 1;").is_empty());
    assert!(names("struct _Marker;").is_empty());
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
fn allow_dead_code_is_inherited_from_the_file() {
    // Rust lint levels are inherited. `#![allow(dead_code)]` at the top of a
    // file covers everything in it, and the documentation promises that
    // `#[allow(dead_code)]` exempts a declaration from DRY-006.
    assert!(names("#![allow(dead_code)]\nstruct Intentional;").is_empty());
}

#[test]
fn allow_dead_code_is_inherited_from_an_enclosing_module() {
    // The generated-code idiom: one attribute on the module, not on each item.
    assert!(names("#[allow(dead_code)]\nmod generated { struct Generated; }").is_empty());
}

#[test]
fn an_allow_on_a_sibling_module_does_not_leak() {
    // The inherited context has to be restored on the way out, or one annotated
    // module would silence the rest of the file.
    let found = detect("#[allow(dead_code)]\nmod a { struct Kept; }\nmod b { struct Dead; }");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].name, "Dead");
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

#[test]
fn allow_dead_code_is_inherited_from_an_enclosing_function() {
    // A lint level is in force for the whole lexical scope, so a local type in
    // an excused function is excused too.
    assert!(names("#[allow(dead_code)]\nfn f() { struct Local; }").is_empty());
}

#[test]
fn an_inner_deny_overrides_an_inherited_allow() {
    // Rust resolves lint levels innermost-first: the file-level allow does not
    // survive a `deny` on the declaration. Modelling inheritance as a one-way
    // flag silently kept suppressing what the author re-armed.
    let found = detect("#![allow(dead_code)]\n#[deny(dead_code)]\nstruct MustBeUsed;");
    assert_eq!(found.len(), 1, "the inner deny wins: {found:?}");
    assert_eq!(found[0].name, "MustBeUsed");
}

#[test]
fn the_last_lint_attribute_in_source_order_wins() {
    // Rust evaluates lint attributes in the order they are written; a later
    // level overrides an earlier one. Scanning a fixed severity order instead
    // reports a declaration the author explicitly allowed.
    assert!(
        names("#[deny(dead_code)]\n#[allow(dead_code)]\nstruct Intentional;").is_empty(),
        "the trailing allow wins"
    );
    assert_eq!(
        names("#[allow(dead_code)]\n#[deny(dead_code)]\nstruct Reported;"),
        vec!["Reported".to_string()],
        "the trailing deny wins"
    );
}

#[test]
fn forbid_cannot_be_downgraded_by_an_inner_allow() {
    // `forbid` is the one level a narrower scope may not relax — for rustc an
    // inner `allow` under it is an error, so honouring the allow would silence
    // something the compiler never would.
    let found = detect("#![forbid(dead_code)]\n#[allow(dead_code)]\nstruct StillReported;");
    assert_eq!(found.len(), 1, "forbid survives the inner allow: {found:?}");
}
