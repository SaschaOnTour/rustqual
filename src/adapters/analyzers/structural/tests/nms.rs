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
fn cfg_test_attr_impl_method_excluded() {
    // An item-level `#[cfg(test)]` impl is test code even in a production file;
    // its methods must not trip NMS (consistent with whole cfg-test files).
    let w = detect_in(
        "struct S { x: i32 } #[cfg(test)] impl S { fn foo(&mut self) -> i32 { self.x } }",
    );
    assert!(
        w.is_empty(),
        "a #[cfg(test)] impl method must be excluded from NMS: {} warning(s)",
        w.len()
    );
}

#[test]
fn cfg_test_attr_method_excluded() {
    // `#[cfg(test)]` directly on an impl METHOD makes that method test code even
    // in a regular production impl — it must not trip NMS.
    let w = detect_in(
        "struct S { x: i32 } impl S { #[cfg(test)] fn foo(&mut self) -> i32 { self.x } }",
    );
    assert!(
        w.is_empty(),
        "a #[cfg(test)] method must be excluded from NMS: {} warning(s)",
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

#[test]
fn test_assign_to_local_still_flagged() {
    // Assigning to a *local* (not `self`) is not a self-mutation, so a method
    // that only reads self is still needless-mut — guards `is_self_target` /
    // the `Expr::Assign` guard against treating every assignment as self-write.
    let w = detect_in(
        "struct S { x: i32 } impl S { fn f(&mut self) -> i32 { let mut y = 0; y = self.x; y } }",
    );
    assert_eq!(w.len(), 1, "assigning a local is not a self-mutation");
}

#[test]
fn test_non_assign_binary_on_self_still_flagged() {
    // `self.x > 0` is a comparison, not a compound assignment — it does not
    // mutate self. Guards `is_compound_assign` and the `&&` in the binary arm.
    let w = detect_in("struct S { x: i32 } impl S { fn f(&mut self) -> bool { self.x > 0 } }");
    assert_eq!(w.len(), 1, "a comparison on self.x is not a mutation");
}

#[test]
fn test_immutable_self_reference_still_flagged() {
    // `&self.x` is an *immutable* borrow — not a mutation. Guards the
    // `r.mutability.is_some() && is_self_target(...)` reference arm.
    let w =
        detect_in("struct S { x: i32 } impl S { fn f(&mut self) -> i32 { let r = &self.x; *r } }");
    assert_eq!(w.len(), 1, "an immutable &self.x borrow is not a mutation");
}

#[test]
fn test_method_call_on_local_still_flagged() {
    // A method call on a *local* (not self) is not a self-mutation. Guards
    // `is_self_field` / `is_self_path` / `is_self_indexed_field` against
    // treating any method-call receiver as self.
    let w = detect_in(
        "struct S { x: i32 } impl S { fn f(&mut self) -> i32 { let v = String::new(); v.len(); self.x } }",
    );
    assert_eq!(
        w.len(),
        1,
        "a method call on a local is not a self-mutation"
    );
}

#[test]
fn test_bare_self_reference_still_flagged() {
    // `self` referenced only as a bare path (passed to a fn) still counts as a
    // self-reference — guards the `Expr::Path` arm of `is_self_ref`.
    let w = detect_in("struct S; impl S { fn f(&mut self) { take(self); } }");
    assert_eq!(w.len(), 1, "a bare `self` argument is a self-reference");
}

#[test]
fn test_non_self_path_not_referenced_as_self() {
    // A non-`self` path (`x`) must NOT be mistaken for a self-reference;
    // without a self-reference NMS defers to SLM. Guards the `== \"self\"`
    // check in `is_self_ref`.
    let w = detect_in("struct S; impl S { fn f(&mut self) { let x = 1; let _ = x; } }");
    assert!(
        w.is_empty(),
        "a non-self local path is not a self-reference"
    );
}

#[test]
fn needless_mut_self_in_non_test_module_flagged() {
    // The inherent-method collector must descend into regular (non-test)
    // modules — guards the `!has_cfg_test_attr` recursion guard in
    // `visit_item_methods` against never descending / only into test modules.
    let w =
        detect_in("mod inner { struct S { x: i32 } impl S { fn f(&mut self) -> i32 { self.x } } }");
    assert_eq!(
        w.len(),
        1,
        "needless &mut self in a non-test module must be flagged"
    );
}

#[test]
fn needless_mut_self_in_inline_cfg_test_module_excluded() {
    // The same inside `#[cfg(test)] mod` must be skipped. `fn keep` keeps the
    // file from being whole-file-classified as test code, so the inline guard
    // is the thing under test.
    let w = detect_in(
        "fn keep() {} #[cfg(test)] mod tests { struct S { x: i32 } impl S { fn f(&mut self) -> i32 { self.x } } }",
    );
    assert!(
        w.is_empty(),
        "needless &mut self in a #[cfg(test)] mod must be excluded"
    );
}

#[test]
fn mutation_through_a_nested_field_counts() {
    // `self.inner.items.insert(v)` mutates self exactly as `self.items.insert(v)`
    // does. Recognising only one field level reports a needless `&mut` on a
    // method that plainly needs it — and NMS's advice would then not compile.
    let w = detect_in(
        "struct Inner { items: Vec<u8> } struct S { inner: Inner } \
         impl S { fn add(&mut self, v: u8) { self.inner.items.push(v); } }",
    );
    assert!(
        w.is_empty(),
        "a nested-field mutation must count as mutating self: {w:?}"
    );
}

#[test]
fn assignment_through_a_nested_field_counts() {
    let w = detect_in(
        "struct Inner { n: u8 } struct S { inner: Inner } \
         impl S { fn set(&mut self, v: u8) { self.inner.n = v; } }",
    );
    assert!(w.is_empty(), "nested assignment mutates self: {w:?}");
}

#[test]
fn a_genuinely_read_only_method_is_still_reported() {
    // The fix must not blunt the check: reading through a nested field is not
    // a mutation.
    let w = detect_in(
        "struct Inner { n: u8 } struct S { inner: Inner } \
         impl S { fn get(&mut self) -> u8 { self.inner.n } }",
    );
    assert_eq!(w.len(), 1, "read-only &mut self is still needless: {w:?}");
    assert!(matches!(w[0].kind, StructuralWarningKind::NeedlessMutSelf));
}
