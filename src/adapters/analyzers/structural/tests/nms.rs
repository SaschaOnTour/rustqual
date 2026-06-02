use crate::adapters::analyzers::structural::nms::*;
use crate::adapters::analyzers::structural::{StructuralWarning, StructuralWarningKind};

fn detect_in(source: &str) -> Vec<StructuralWarning> {
    super::detect_single(source, detect_nms)
}

#[test]
fn needless_mut_self_in_cfg_test_file_excluded() {
    // NMS must skip whole test files (here a `#![cfg(test)]` companion),
    // not only inline `#[cfg(test)] mod`.
    let w = detect_in(
        "#![cfg(test)]\nstruct S { x: i32 } impl S { fn foo(&mut self) -> i32 { self.x } }",
    );
    assert!(
        w.is_empty(),
        "needless &mut self in a #![cfg(test)] file must be excluded: {} warning(s)",
        w.len()
    );
}

#[test]
fn test_needless_mut_self_flagged() {
    let w = detect_in("struct S { x: i32 } impl S { fn foo(&mut self) -> i32 { self.x } }");
    assert_eq!(w.len(), 1);
    assert!(matches!(w[0].kind, StructuralWarningKind::NeedlessMutSelf));
}

#[test]
fn test_assignment_not_flagged() {
    let w = detect_in("struct S { x: i32 } impl S { fn set(&mut self, v: i32) { self.x = v; } }");
    assert!(w.is_empty());
}

#[test]
fn test_method_call_on_self_not_flagged() {
    let w = detect_in(
        "struct S { items: Vec<i32> } impl S { fn add(&mut self, v: i32) { self.items.push(v); } }",
    );
    assert!(w.is_empty());
}

#[test]
fn test_mut_borrow_not_flagged() {
    let w = detect_in(
        "struct S { x: i32 } impl S { fn borrow(&mut self) -> &mut i32 { &mut self.x } }",
    );
    assert!(w.is_empty());
}

#[test]
fn test_immutable_self_not_checked() {
    let w = detect_in("struct S { x: i32 } impl S { fn foo(&self) -> i32 { self.x } }");
    assert!(w.is_empty());
}

#[test]
fn test_trait_impl_excluded() {
    let w = detect_in("trait T { fn foo(&mut self); } struct S { x: i32 } impl T for S { fn foo(&mut self) { let _ = self.x; } }");
    assert!(w.is_empty());
}

#[test]
fn test_no_self_ref_skipped_for_slm() {
    // If self is never referenced, SLM catches it — NMS should not fire
    let w = detect_in("struct S; impl S { fn foo(&mut self) -> i32 { 42 } }");
    assert!(w.is_empty());
}

#[test]
fn test_empty_body_not_flagged() {
    let w = detect_in("struct S; impl S { fn foo(&mut self) {} }");
    assert!(w.is_empty());
}

#[test]
fn test_indexed_field_method_call_not_flagged() {
    let w = detect_in(
        "struct S { items: Vec<Vec<i32>> } impl S { fn add(&mut self, i: usize, v: i32) { self.items[i].push(v); } }",
    );
    assert!(
        w.is_empty(),
        "self.items[i].push(v) should be recognized as mutation"
    );
}
