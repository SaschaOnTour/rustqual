use crate::adapters::analyzers::structural::slm::*;
use crate::adapters::analyzers::structural::{StructuralWarning, StructuralWarningKind};

fn detect_in(source: &str) -> Vec<StructuralWarning> {
    super::detect_single(source, detect_slm)
}

#[test]
fn selfless_method_in_cfg_test_file_excluded() {
    // SLM must skip whole test files (here a `#![cfg(test)]` companion),
    // not only inline `#[cfg(test)] mod`.
    let w = detect_in("#![cfg(test)]\nstruct S; impl S { fn foo(&self) -> i32 { 42 } }");
    assert!(
        w.is_empty(),
        "selfless method in a #![cfg(test)] file must be excluded: {} warning(s)",
        w.len()
    );
}

#[test]
fn cfg_test_attr_impl_method_excluded() {
    // An item-level `#[cfg(test)]` impl is test code even in a production file;
    // its methods must not trip SLM (consistent with whole cfg-test files).
    let w = detect_in("struct S; #[cfg(test)] impl S { fn foo(&self) -> i32 { 42 } }");
    assert!(
        w.is_empty(),
        "a #[cfg(test)] impl method must be excluded from SLM: {} warning(s)",
        w.len()
    );
}

#[test]
fn cfg_test_attr_method_excluded() {
    // `#[cfg(test)]` directly on an impl METHOD makes that method test code even
    // in a regular production impl — it must not trip SLM.
    let w = detect_in("struct S; impl S { #[cfg(test)] fn foo(&self) -> i32 { 42 } }");
    assert!(
        w.is_empty(),
        "a #[cfg(test)] method must be excluded from SLM: {} warning(s)",
        w.len()
    );
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

#[test]
fn test_self_in_macro_arg_not_flagged() {
    // A method whose ONLY self-reference is a macro argument (here `format!`)
    // genuinely uses self — it must not trip SLM. `syn::visit` treats a macro
    // body as an opaque token stream, so the `SelfRefChecker` has to scan macro
    // tokens for `self` (it special-cases only `matches!` today → false positive).
    let w = detect_in(
        "struct S { name: String } impl S { fn label(&self) -> String { format!(\"[{}]\", self.name) } }",
    );
    assert!(
        w.is_empty(),
        "self inside a macro arg (format!) must count as a self reference: {} warning(s)",
        w.len()
    );
}

#[test]
fn self_in_an_inline_format_arg_not_flagged() {
    // `format!("{self:?}")` uses self just as `format!("{:?}", self)` does, but
    // proc_macro2 lexes the whole string as ONE literal — so a token scan sees
    // no `self` at all and the method looks self-less. Inline format arguments
    // have been idiomatic since Rust 2021.
    let w = detect_in(
        "struct S { n: u8 } impl S { fn label(&self) -> String { format!(\"{self:?}\") } }",
    );
    assert!(
        w.is_empty(),
        "self interpolated into a format string is a self reference: {w:?}"
    );
}
