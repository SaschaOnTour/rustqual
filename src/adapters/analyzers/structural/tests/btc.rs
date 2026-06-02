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
