use crate::adapters::analyzers::structural::btc::*;
use crate::adapters::analyzers::structural::{StructuralWarning, StructuralWarningKind};
use crate::config::StructuralConfig;

fn detect_in(source: &str) -> Vec<StructuralWarning> {
    super::detect_single(source, detect_btc)
}

#[test]
fn stub_impl_in_cfg_test_file_excluded() {
    // BTC must skip whole test files (here a `#![cfg(test)]` companion),
    // not only inline `#[cfg(test)] mod` — mock impls with `todo!()`
    // stubs are legitimate in tests.
    let w = detect_in(
        "#![cfg(test)]\ntrait Foo { fn bar(&self); } impl Foo for MyType { fn bar(&self) { todo!() } } struct MyType;",
    );
    assert!(
        w.is_empty(),
        "stub impl in a #![cfg(test)] file must be excluded: {} warning(s)",
        w.len()
    );
}

#[test]
fn test_all_stub_methods_flagged() {
    let w = detect_in("trait Foo { fn bar(&self); } impl Foo for MyType { fn bar(&self) { todo!() } } struct MyType;");
    assert_eq!(w.len(), 1);
    assert!(matches!(
        w[0].kind,
        StructuralWarningKind::BrokenTraitContract { .. }
    ));
}

#[test]
fn test_unimplemented_flagged() {
    let w = detect_in("trait Foo { fn bar(&self); } impl Foo for MyType { fn bar(&self) { unimplemented!() } } struct MyType;");
    assert_eq!(w.len(), 1);
}

#[test]
fn test_panic_not_implemented_flagged() {
    let w = detect_in("trait Foo { fn bar(&self); } impl Foo for MyType { fn bar(&self) { panic!(\"not implemented\") } } struct MyType;");
    assert_eq!(w.len(), 1);
}

#[test]
fn test_real_impl_not_flagged() {
    let w = detect_in("trait Foo { fn bar(&self) -> i32; } impl Foo for MyType { fn bar(&self) -> i32 { 42 } } struct MyType;");
    assert!(w.is_empty());
}

#[test]
fn test_inherent_impl_not_flagged() {
    let w = detect_in("struct MyType; impl MyType { fn bar(&self) { todo!() } }");
    assert!(w.is_empty());
}

#[test]
fn test_default_default_body_not_treated_as_stub() {
    // BTC stub forms are the panic-style macros only — `todo!`,
    // `unimplemented!`, `panic!("not implemented")` (see `is_stub_body` and
    // `book/function-quality.md`). A method whose body is `Default::default()`
    // is frequently a genuine implementation, so it is intentionally NOT a stub.
    // Pin that contract so any future change to flag it is deliberate.
    let w = detect_in(
        "trait Foo { fn bar(&self) -> i32; } impl Foo for M { fn bar(&self) -> i32 { Default::default() } } struct M;",
    );
    assert!(
        w.is_empty(),
        "a Default::default() body is not treated as a BTC stub"
    );
}

#[test]
fn test_empty_impl_not_flagged() {
    let w = detect_in("trait Foo {} impl Foo for MyType {} struct MyType;");
    assert!(w.is_empty());
}

#[test]
fn test_partial_stub_flags_only_stubs() {
    let w = detect_in("trait Foo { fn a(&self); fn b(&self) -> i32; } impl Foo for M { fn a(&self) { todo!() } fn b(&self) -> i32 { 42 } } struct M;");
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].name, "a");
}

#[test]
fn stub_impl_in_non_test_module_flagged() {
    // A stub trait impl nested in a regular (non-test) module must still be
    // found — guards the recursion guard against never descending / descending
    // only into test modules.
    let w = detect_in(
        "mod inner { trait Foo { fn bar(&self); } impl Foo for MyType { fn bar(&self) { todo!() } } struct MyType; }",
    );
    assert_eq!(w.len(), 1, "stub impl in a non-test module must be flagged");
}

#[test]
fn stub_impl_in_inline_cfg_test_module_excluded() {
    // The same stub inside `#[cfg(test)] mod` must be skipped. `struct Keep`
    // stops the file being whole-file-classified as test code, so the inline
    // `!has_cfg_test_attr` guard is the thing under test.
    let w = detect_in(
        "struct Keep; #[cfg(test)] mod tests { trait Foo { fn bar(&self); } impl Foo for MyType { fn bar(&self) { todo!() } } struct MyType; }",
    );
    assert!(
        w.is_empty(),
        "stub impl in a #[cfg(test)] mod must be excluded"
    );
}

#[test]
fn test_disabled_check() {
    let syntax = syn::parse_file(
        "trait Foo { fn bar(&self); } impl Foo for M { fn bar(&self) { todo!() } } struct M;",
    )
    .expect("test source");
    let parsed = vec![("test.rs".to_string(), String::new(), syntax)];
    let config = StructuralConfig {
        check_btc: false,
        ..StructuralConfig::default()
    };
    let mut warnings = Vec::new();
    detect_btc(
        &mut warnings,
        &parsed,
        &config,
        &std::collections::HashSet::new(),
    );
    assert!(warnings.is_empty());
}
