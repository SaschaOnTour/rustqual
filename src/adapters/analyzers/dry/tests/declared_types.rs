//! Collecting the declarations DRY-006 judges: types proper plus constants.

use crate::adapters::analyzers::dry::collect_declared_types;
use crate::adapters::shared::declared_type::{DeclaredType, TypeItemKind};

fn declared(code: &str) -> Vec<DeclaredType> {
    let syntax = syn::parse_file(code).expect("fixture must parse");
    collect_declared_types(
        &[("src/lib.rs".to_string(), code.to_string(), syntax)],
        &std::collections::HashSet::new(),
    )
}

fn kinds_of(code: &str) -> Vec<(String, TypeItemKind)> {
    declared(code)
        .into_iter()
        .map(|d| (d.name, d.kind))
        .collect()
}

#[test]
fn every_declaration_kind_is_collected_with_its_own_label() {
    // One mechanism covers all six shapes; the kind rides along because the
    // message has to name what it found ("struct `Foo` …", "const `MAX` …").
    let code = "pub struct S; enum E {} union U { a: u8 } type A = u8; \
                const C: u8 = 1; static T: u8 = 1;";
    assert_eq!(
        kinds_of(code),
        vec![
            ("S".to_string(), TypeItemKind::Struct),
            ("E".to_string(), TypeItemKind::Enum),
            ("U".to_string(), TypeItemKind::Union),
            ("A".to_string(), TypeItemKind::TypeAlias),
            ("C".to_string(), TypeItemKind::Const),
            ("T".to_string(), TypeItemKind::Static),
        ]
    );
}

#[test]
fn functions_and_traits_are_not_collected() {
    // Functions are DRY-002's job. Traits are deliberately out of scope: every
    // `impl Trait for X` names the trait, so a trait with one implementation
    // would always look used — the check would carry the risk without the value.
    assert!(
        kinds_of("fn f() {} trait T { fn m(&self); } impl T for u8 { fn m(&self) {} }").is_empty()
    );
}

#[test]
fn the_declaration_records_its_own_line() {
    let d = declared("\n\nstruct Late;");
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].line, 3, "anchored at the declaration, not the file");
}

#[test]
fn allow_dead_code_is_recorded() {
    // The author already told the compiler this is intentional; DRY-006 must
    // not contradict it.
    let d = declared("#[allow(dead_code)]\nstruct Kept;");
    assert!(d[0].dead_code_exempt);
}

#[test]
fn a_declaration_in_a_cfg_test_module_is_marked_as_test() {
    // Test-only declarations are test code by their own right, exactly as
    // DRY-002 treats test functions.
    let d = declared("#[cfg(test)]\nmod tests { struct Fixture; }");
    assert_eq!(d.len(), 1);
    assert!(d[0].is_test, "declared inside a #[cfg(test)] module");
}

#[test]
fn associated_consts_are_not_top_level_declarations() {
    // `impl Foo { const MAX: u8 = 1; }` is an associated item — reachable only
    // through its type, which DRY-006 already judges.
    assert!(kinds_of("struct Foo; impl Foo { const MAX: u8 = 1; }")
        .iter()
        .all(|(name, _)| name != "MAX"));
}
