//! Own-call recording: lenient nested contexts, recursion, iterator skip,
//! receiver type resolution, and trivial-self-getter exclusion. Each fixture
//! has `f` call something; whether it is recorded as an own call reveals the
//! decision under test. Shared helpers live in the parent module.
use super::*;

#[test]
fn async_block_body_is_lenient() {
    // An own call inside an `async` block is skipped (lenient, like a closure)
    // under the default config — guards the `Expr::Async` arm that enters
    // lenient mode.
    let calls = own_calls_of("fn g() {} fn f() { let _ = async { g() }; }", "f");
    assert!(
        calls.is_empty(),
        "calls inside async are lenient, got {calls:?}"
    );
}

#[test]
fn async_depth_pops_after_block() {
    // After an `async` block the leniency must be popped, so a following own
    // call is still counted — guards the `async_block_depth -= 1` restore.
    let calls = own_calls_of("fn g() {} fn f() { let _ = async {}; g(); }", "f");
    assert!(
        !calls.is_empty(),
        "an own call after an async block must still count, got {calls:?}"
    );
}

#[test]
fn non_iterator_own_method_call_is_counted() {
    // A non-iterator own method call must be recorded as an own call. Guards the
    // `is_iterator && !strict_iterator_chains` skip condition.
    let calls = own_calls_of(
        "struct S; impl S { fn helper(&self) {} } fn f(s: S) { s.helper(); let _ = 0; }",
        "f",
    );
    assert!(
        calls.iter().any(|c| c.contains("helper")),
        "a non-iterator own method call is an own call, got {calls:?}"
    );
}

#[test]
fn recursive_own_method_call_is_counted_when_recursion_disallowed() {
    // With `allow_recursion = false` (default), a self-recursive own method call
    // is still an own call: `allow_recursion && is_recursive` short-circuits to
    // false. Turning `&&` into `||` would skip recursive calls.
    let calls = own_calls_of(
        "struct S; impl S { fn f(&self) -> i32 { let _ = 0; self.f() } }",
        "f",
    );
    assert!(
        calls.iter().any(|c| c.contains("f")),
        "a recursive own method call counts when recursion is disallowed, got {calls:?}"
    );
}

// --- receiver type resolution -------------------------------------------
// `foo` is a method of `A`; resolving the receiver's type distinguishes a call
// on a `B` value (not own) from the name-only fallback (which would accept it).

#[test]
fn receiver_of_wrong_type_is_not_an_own_call() {
    // Guards `extract_simple_type` (Path arm / `-> None`) and
    // `resolve_receiver_type`'s non-self Path arm.
    let calls = own_calls_of(
        "struct A; struct B; impl A { fn foo(&self) {} fn f(&self, b: B) { b.foo(); let _ = 0; } }",
        "f",
    );
    assert!(
        calls.is_empty(),
        "a method call on a wrong-typed receiver is not an own call, got {calls:?}"
    );
}

#[test]
fn self_receiver_resolves_to_parent_type_only() {
    // In `B::f`, `self.foo()` resolves to `B` (no `foo`), so it is not an own
    // call — guards the `is_ident("self")` arm of `resolve_receiver_type`.
    let calls = own_calls_of(
        "struct A; impl A { fn foo(&self) {} } struct B; impl B { fn f(&self) { self.foo(); let _ = 0; } }",
        "f",
    );
    assert!(
        calls.is_empty(),
        "self.foo() resolves to the parent type only, got {calls:?}"
    );
}

#[test]
fn reference_typed_receiver_is_resolved() {
    // A `&B` parameter must resolve to `B` (references are unwrapped). Guards the
    // `Type::Reference` arm of `extract_simple_type`.
    let calls = own_calls_of(
        "struct A; struct B; impl A { fn foo(&self) {} fn f(&self, b: &B) { b.foo(); let _ = 0; } }",
        "f",
    );
    assert!(
        calls.is_empty(),
        "a &B receiver resolves to B; A::foo on it is not an own call, got {calls:?}"
    );
}

// --- trivial self-getter detection (scope.rs) ---------------------------
// A method recognised as a trivial self-getter is added to `trivial_methods`
// and excluded from own-call counting. Each fixture has `f` call `g`; whether
// `g` counts as an own call of `f` reveals whether `g` was classified trivial.

#[test]
fn method_with_extra_param_is_not_a_trivial_getter() {
    // `has_trivial_self_signature`: a method with a non-self parameter is not a
    // trivial getter, so the call to it counts. Guards `has_receiver &&
    // typed_count == 0` (→ true, and `&&` → `||`).
    let calls = own_calls_of(
        "struct S { x: i32 } impl S { fn g(&self, n: i32) -> i32 { self.x } fn f(&self) -> i32 { self.g(1) } }",
        "f",
    );
    assert!(
        calls.iter().any(|c| c.contains("g")),
        "a call to a non-trivial (extra-param) method is an own call, got {calls:?}"
    );
}

#[test]
fn cast_accessor_body_is_a_trivial_getter() {
    // `self.x as f64` is a trivial accessor; guards the `Expr::Cast` arm.
    let calls = own_calls_of(
        "struct S { x: i32 } impl S { fn g(&self) -> f64 { self.x as f64 } fn f(&self) -> f64 { self.g() } }",
        "f",
    );
    assert!(
        calls.is_empty(),
        "a cast-accessor getter call is not an own call, got {calls:?}"
    );
}

#[test]
fn unary_accessor_body_is_a_trivial_getter() {
    // `-self.x` is a trivial accessor; guards the `Expr::Unary` arm.
    let calls = own_calls_of(
        "struct S { x: i32 } impl S { fn g(&self) -> i32 { -self.x } fn f(&self) -> i32 { self.g() } }",
        "f",
    );
    assert!(
        calls.is_empty(),
        "a unary-accessor getter call is not an own call, got {calls:?}"
    );
}

#[test]
fn paren_accessor_body_is_a_trivial_getter() {
    // `(self.x)` is a trivial accessor; guards the `Expr::Paren` arm.
    let calls = own_calls_of(
        "struct S { x: i32 } impl S { fn g(&self) -> i32 { (self.x) } fn f(&self) -> i32 { self.g() } }",
        "f",
    );
    assert!(
        calls.is_empty(),
        "a paren-accessor getter call is not an own call, got {calls:?}"
    );
}

#[test]
fn non_get_one_arg_method_breaks_trivial_accessor() {
    // Only a no-arg stdlib accessor or `get` with a trivial arg is trivial. A
    // one-arg non-`get` call (`self.items.bar(0)`) makes the body non-trivial,
    // so the getter is NOT trivial. Guards the `args.len() != 1 || method !=
    // "get"` early-out.
    let calls = own_calls_of(
        "struct S { items: Vec<i32> } impl S { fn g(&self) -> i32 { self.items.bar(0) } fn f(&self) -> i32 { self.g() } }",
        "f",
    );
    assert!(
        calls.iter().any(|c| c.contains("g")),
        "a one-arg non-get call is not a trivial accessor, so g is an own call, got {calls:?}"
    );
}

#[test]
fn get_with_non_self_path_arg_breaks_trivial_accessor() {
    // `self.items.get(IDX)` — the `get` argument is a non-self path (a const),
    // which is not a trivial arg, so the body is not a trivial accessor and the
    // getter counts. `g` takes no extra parameter (so its *signature* is trivial)
    // — the body check is therefore decisive. Guards the `is_ident("self")` arg
    // guard against accepting any path.
    let calls = own_calls_of(
        "const IDX: usize = 0; struct S { items: Vec<i32> } impl S { fn g(&self) -> i32 { self.items.get(IDX) } fn f(&self) -> i32 { self.g() } }",
        "f",
    );
    assert!(
        calls.iter().any(|c| c.contains("g")),
        "get(non_self_path) is not a trivial accessor, so g is an own call, got {calls:?}"
    );
}
