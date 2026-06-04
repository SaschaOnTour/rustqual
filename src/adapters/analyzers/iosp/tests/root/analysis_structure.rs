//! Whole-function analysis: effort-score arithmetic, severity boundary,
//! function-line count, for-loop/match delegation, nested-module recursion and
//! the `#[cfg(test)] impl` test flag. Shared helpers live in the parent module.
use super::*;

#[test]
fn violation_effort_score_is_the_weighted_sum() {
    // A Violation with 1 logic location, 1 own call and nesting depth 1 has
    // effort = 1*1.0 + 1*1.5 + 1*2.0 = 4.5. Pins the whole weighted-sum formula
    // (`build_function_analysis`), killing every `*`/`+` arithmetic mutant.
    let a = raw_analysis_of("fn g() {} fn f() { if true { g(); } }", "f");
    assert_eq!(a.effort_score, Some(4.5), "effort = 1*1.0 + 1*1.5 + 1*2.0");
}

#[test]
fn violation_with_total_two_is_low_severity() {
    // `compute_severity`: total (logic + calls) = 2 is NOT `> 2`, so it is Low,
    // not Medium. Guards the `> SEVERITY_MEDIUM_THRESHOLD` boundary (`>=`).
    let a = raw_analysis_of("fn g() {} fn f() { if true { g(); } }", "f");
    assert_eq!(
        a.severity,
        Some(Severity::Low),
        "total == 2 is Low, not Medium"
    );
}

#[test]
fn violation_with_total_five_is_medium_severity() {
    // total = 4 logic + 1 call = 5, which is NOT `> 5`, so it is Medium, not
    // High. Guards the `> SEVERITY_HIGH_THRESHOLD` boundary (`>` vs `>=`).
    let a = raw_analysis_of(
        "fn g() {} fn f() { if true {} if true {} if true {} if true {} g(); }",
        "f",
    );
    assert_eq!(
        a.severity,
        Some(Severity::Medium),
        "total == 5 is Medium, not High"
    );
}

#[test]
fn single_line_function_has_one_line_count() {
    // `function_lines = close_line.saturating_sub(open_line) + 1`. A one-line
    // body spans exactly 1 line; guards the `+ 1` (turning it into `* 1` gives
    // 0). The `if` makes the body non-trivial so metrics are recorded.
    let m = metrics_of("fn f() { if true { let _ = 1; } }");
    assert_eq!(m.function_lines, 1, "a single-line body is 1 line");
}

// --- for-loop / match delegation (visitor/mod.rs) -----------------------
// A for-loop whose body only delegates (calls, possibly via `return`/parens)
// is dispatch, not logic, so it records no logic. Deleting a delegation arm
// makes the body "not delegation" → the loop wrongly counts as logic.

#[test]
fn for_loop_with_return_delegation_is_not_logic() {
    // `return g()` is delegation; guards the `Expr::Return` arm of
    // `check_delegation_stack`.
    let m = metrics_of("fn g() {} fn f() { for _ in 0..1 { return g(); } }");
    assert_eq!(
        m.logic_count, 0,
        "a return-delegating for body is not logic"
    );
}

#[test]
fn for_loop_with_paren_delegation_is_not_logic() {
    // `(g())` is delegation; guards the `Expr::Paren` arm of
    // `check_delegation_stack`.
    let m = metrics_of("fn g() {} fn f() { for _ in 0..1 { (g()); } }");
    assert_eq!(m.logic_count, 0, "a paren-delegating for body is not logic");
}

#[test]
fn match_with_block_arms_keeps_them_trivial() {
    // Single-statement block arms (`=> { 0 }`) are trivial, so they don't add to
    // cyclomatic complexity (base stays 1). Guards the `stmts.len() == 1` guard
    // in `is_trivial_match_arm` (turning it false would count them).
    let m = metrics_of("fn f(x: i32) -> i32 { match x { 1 => { 0 }, _ => { 1 } } }");
    assert_eq!(
        m.cyclomatic_complexity, 1,
        "single-statement block match arms are trivial"
    );
}

// --- nested-module recursion (mod.rs analyze_mod) -----------------------
// Items inside nested modules must still be analysed and appear in the results.
// Deleting a recursion arm drops them entirely.

#[test]
fn impl_method_in_nested_module_is_analysed() {
    // Guards the `Item::Impl` arm of `analyze_mod`.
    let names = analysed_names("mod inner { struct S; impl S { fn m(&self) {} } }");
    assert!(names.iter().any(|n| n == "m"), "got {names:?}");
}

#[test]
fn trait_method_in_nested_module_is_analysed() {
    // Guards the `Item::Trait` arm of `analyze_mod`.
    let names = analysed_names("mod inner { trait T { fn m(&self) {} } }");
    assert!(names.iter().any(|n| n == "m"), "got {names:?}");
}

#[test]
fn function_in_doubly_nested_module_is_analysed() {
    // Guards the `Item::Mod` arm of `analyze_mod` (recursion into deeper mods).
    let names = analysed_names("mod inner { mod deeper { fn m() {} } }");
    assert!(names.iter().any(|n| n == "m"), "got {names:?}");
}

// --- cfg(test) impl marks methods as test code -------------------------
// `file_in_test || has_cfg_test(impl.attrs)` (and the `mod_is_test || …`
// mirror) — a `#[cfg(test)] impl` in production code marks its methods test,
// even though the file/module itself is not test. `struct Keep` stops the file
// being whole-file-classified, so the per-impl `||` is what's exercised.

#[test]
fn method_in_top_level_cfg_test_impl_is_marked_test() {
    // Guards `analyze_file`'s Impl-arm `file_in_test || has_cfg_test(&i.attrs)`.
    let m = analysis_of(
        "struct Keep; struct S; #[cfg(test)] impl S { fn m(&self) {} }",
        "m",
    );
    assert!(m.is_test, "a method in a #[cfg(test)] impl is test code");
}

#[test]
fn default_method_in_top_level_cfg_test_trait_is_marked_test() {
    // Guards `analyze_file`'s Trait-arm `file_in_test || has_cfg_test(&t.attrs)`.
    let m = analysis_of("struct Keep; #[cfg(test)] trait T { fn m(&self) {} }", "m");
    assert!(
        m.is_test,
        "a default method in a #[cfg(test)] trait is test code"
    );
}

#[test]
fn method_in_modular_cfg_test_impl_is_marked_test() {
    // Guards `analyze_mod`'s `mod_is_test || has_cfg_test(&i.attrs)`.
    let m = analysis_of(
        "struct Keep; struct S; mod inner { #[cfg(test)] impl super::S { fn m(&self) {} } }",
        "m",
    );
    assert!(
        m.is_test,
        "a method in a nested #[cfg(test)] impl is test code"
    );
}
