use crate::adapters::analyzers::structural::deh::*;
use crate::adapters::analyzers::structural::{StructuralWarning, StructuralWarningKind};
use crate::config::StructuralConfig;
use syn::visit::Visit;

fn detect_in(source: &str) -> Vec<StructuralWarning> {
    let syntax = syn::parse_file(source).expect("test source");
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let config = StructuralConfig::default();
    let mut warnings = Vec::new();
    detect_deh(&mut warnings, &parsed, &config);
    warnings
}

#[test]
fn test_downcast_ref_flagged() {
    let w = detect_in("fn foo(a: &dyn std::any::Any) { a.downcast_ref::<i32>(); }");
    assert_eq!(w.len(), 1);
    assert!(matches!(
        w[0].kind,
        StructuralWarningKind::DowncastEscapeHatch
    ));
}

#[test]
fn test_downcast_mut_flagged() {
    let w = detect_in("fn foo(a: &mut dyn std::any::Any) { a.downcast_mut::<i32>(); }");
    assert_eq!(w.len(), 1);
}

#[test]
fn test_no_downcast_not_flagged() {
    let w = detect_in("fn foo() { let x = 42; }");
    assert!(w.is_empty());
}

#[test]
fn test_test_code_excluded() {
    let w = detect_in(
        "#[cfg(test)] mod tests { fn foo(a: &dyn std::any::Any) { a.downcast_ref::<i32>(); } }",
    );
    assert!(w.is_empty());
}

#[test]
fn test_disabled_check() {
    let syntax = syn::parse_file("fn foo(a: &dyn std::any::Any) { a.downcast_ref::<i32>(); }")
        .expect("test source");
    let parsed = vec![("test.rs".to_string(), String::new(), syntax)];
    let config = StructuralConfig {
        check_deh: false,
        ..StructuralConfig::default()
    };
    let mut warnings = Vec::new();
    detect_deh(&mut warnings, &parsed, &config);
    assert!(warnings.is_empty());
}
