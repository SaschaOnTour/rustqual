use crate::adapters::analyzers::structural::iet::*;
use crate::adapters::analyzers::structural::{StructuralWarning, StructuralWarningKind};
use crate::config::StructuralConfig;

fn detect_in(source: &str) -> Vec<StructuralWarning> {
    super::detect_single(source, detect_iet)
}

#[test]
fn inconsistent_error_types_in_cfg_test_file_excluded() {
    // IET must skip whole test files (here a `#![cfg(test)]` companion),
    // not only inline `#[cfg(test)] mod`.
    let w = detect_in(
        "#![cfg(test)]\npub fn a() -> Result<(), String> { Ok(()) } pub fn b() -> Result<i32, std::io::Error> { todo!() }",
    );
    assert!(
        w.is_empty(),
        "inconsistent error types in a #![cfg(test)] file must be excluded: {} warning(s)",
        w.len()
    );
}

#[test]
fn test_consistent_error_types_not_flagged() {
    let w = detect_in(
        "pub fn a() -> Result<(), String> { Ok(()) } pub fn b() -> Result<i32, String> { Ok(1) }",
    );
    assert!(w.is_empty());
}

#[test]
fn test_inconsistent_error_types_flagged() {
    let w = detect_in("pub fn a() -> Result<(), String> { Ok(()) } pub fn b() -> Result<i32, std::io::Error> { Ok(1) }");
    assert_eq!(w.len(), 1);
    assert!(matches!(
        w[0].kind,
        StructuralWarningKind::InconsistentErrorTypes { .. }
    ));
}

#[test]
fn test_single_pub_fn_not_flagged() {
    let w = detect_in("pub fn a() -> Result<(), String> { Ok(()) }");
    assert!(w.is_empty());
}

#[test]
fn test_private_fns_not_counted() {
    let w = detect_in("pub fn a() -> Result<(), String> { Ok(()) } fn b() -> Result<i32, std::io::Error> { Ok(1) }");
    assert!(w.is_empty());
}

#[test]
fn test_no_result_return_not_counted() {
    let w = detect_in("pub fn a() -> Result<(), String> { Ok(()) } pub fn b() -> i32 { 1 }");
    assert!(w.is_empty());
}

#[test]
fn test_normalized_paths() {
    // std::io::Error and io::Error should be the same
    let w = detect_in("pub fn a() -> Result<(), io::Error> { todo!() } pub fn b() -> Result<(), std::io::Error> { todo!() }");
    assert!(
        w.is_empty(),
        "std:: prefix should be stripped for comparison"
    );
}

#[test]
fn test_disabled_check() {
    let syntax = syn::parse_file("pub fn a() -> Result<(), String> { Ok(()) } pub fn b() -> Result<i32, std::io::Error> { Ok(1) }").expect("test source");
    let parsed = vec![("lib.rs".to_string(), String::new(), syntax)];
    let config = StructuralConfig {
        check_iet: false,
        ..StructuralConfig::default()
    };
    let mut warnings = Vec::new();
    detect_iet(
        &mut warnings,
        &parsed,
        &config,
        &std::collections::HashSet::new(),
    );
    assert!(warnings.is_empty());
}

#[test]
fn test_cfg_test_module_excluded() {
    let w = detect_in("#[cfg(test)] mod tests { pub fn a() -> Result<(), String> { Ok(()) } pub fn b() -> Result<i32, std::io::Error> { Ok(1) } }");
    assert!(w.is_empty());
}

#[test]
fn inconsistent_error_types_across_non_test_module_flagged() {
    // One conflicting error type lives in a regular (non-test) module; the
    // collector must descend into it — guards the module-recursion guard
    // against being skipped (otherwise only one error type is seen).
    let w = detect_in(
        "pub fn a() -> Result<(), String> { Ok(()) } mod inner { pub fn b() -> Result<i32, std::io::Error> { Ok(1) } }",
    );
    assert_eq!(
        w.len(),
        1,
        "an error type inside a non-test module must be collected"
    );
}
