use crate::adapters::analyzers::structural::deh::*;
use crate::adapters::analyzers::structural::{StructuralWarning, StructuralWarningKind};
use crate::config::StructuralConfig;

fn detect_in(source: &str) -> Vec<StructuralWarning> {
    let parsed = super::parse_single(source);
    let config = StructuralConfig::default();
    let cfg_test_files =
        crate::adapters::shared::cfg_test_files::collect_cfg_test_file_paths(&parsed);
    let mut warnings = Vec::new();
    detect_deh(&mut warnings, &parsed, &config, &cfg_test_files);
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
fn downcast_in_cfg_test_companion_file_excluded() {
    // DEH (Coupling) must skip test code identified by the authoritative
    // cfg-test set, not only inline `#[cfg(test)] mod`. A `#![cfg(test)]`
    // whole-file companion (and package integration-test files) count as
    // test code even without an inline cfg(test) module or `#[test]` fn.
    let w =
        detect_in("#![cfg(test)]\nfn helper(a: &dyn std::any::Any) { a.downcast_ref::<i32>(); }");
    assert!(
        w.is_empty(),
        "downcast in a #![cfg(test)] file must be excluded: {} warning(s)",
        w.len()
    );
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
    let cfg_test_files = std::collections::HashSet::new();
    let mut warnings = Vec::new();
    detect_deh(&mut warnings, &parsed, &config, &cfg_test_files);
    assert!(warnings.is_empty());
}
