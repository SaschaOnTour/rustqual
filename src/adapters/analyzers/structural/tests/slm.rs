use crate::adapters::analyzers::structural::slm::*;
use crate::adapters::analyzers::structural::{StructuralWarning, StructuralWarningKind};
use crate::config::StructuralConfig;

fn detect_in(source: &str) -> Vec<StructuralWarning> {
    let parsed = super::parse_single(source);
    let config = StructuralConfig::default();
    let mut warnings = Vec::new();
    detect_slm(&mut warnings, &parsed, &config);
    warnings
}

#[test]
fn test_selfless_method_flagged() {
    let w = detect_in("struct S; impl S { fn foo(&self) -> i32 { 42 } }");
    assert_eq!(w.len(), 1);
    assert!(matches!(w[0].kind, StructuralWarningKind::SelflessMethod));
}

#[test]
fn test_self_field_access_not_flagged() {
    let w = detect_in("struct S { x: i32 } impl S { fn foo(&self) -> i32 { self.x } }");
    assert!(w.is_empty());
}

#[test]
fn test_self_method_call_not_flagged() {
    let w = detect_in("struct S; impl S { fn foo(&self) -> String { self.to_string() } }");
    assert!(w.is_empty());
}

#[test]
fn test_trait_impl_excluded() {
    let w = detect_in(
        "trait T { fn foo(&self) -> i32; } struct S; impl T for S { fn foo(&self) -> i32 { 42 } }",
    );
    assert!(w.is_empty());
}

#[test]
fn test_no_receiver_not_flagged() {
    let w = detect_in("struct S; impl S { fn new() -> Self { S } }");
    assert!(w.is_empty());
}

#[test]
fn test_empty_body_not_flagged() {
    let w = detect_in("struct S; impl S { fn foo(&self) {} }");
    assert!(w.is_empty());
}

#[test]
fn test_stub_body_not_flagged() {
    let w = detect_in("struct S; impl S { fn foo(&self) { todo!() } }");
    assert!(w.is_empty());
}

#[test]
fn test_mut_self_selfless_flagged() {
    let w = detect_in("struct S; impl S { fn foo(&mut self) -> i32 { 42 } }");
    assert_eq!(w.len(), 1);
}

#[test]
fn test_matches_macro_self_not_flagged() {
    let w = detect_in(
        "struct S { x: bool } impl S { fn foo(&self) -> bool { matches!(self, S { x: true }) } }",
    );
    assert!(
        w.is_empty(),
        "matches!(self, ...) should count as self reference"
    );
}
