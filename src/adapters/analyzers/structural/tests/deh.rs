use crate::adapters::analyzers::structural::deh::*;
use crate::adapters::analyzers::structural::{StructuralWarning, StructuralWarningKind};
use crate::config::StructuralConfig;

fn detect_in(source: &str) -> Vec<StructuralWarning> {
    super::detect_single(source, detect_deh)
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
fn downcast_in_test_fn_excluded() {
    // A `#[test]` fn is test code regardless of its enclosing module/file cfg
    // state — its body's downcast is a legitimate test escape hatch. This is
    // also the shape the macro-expansion pre-pass emits for `quickcheck!` /
    // `proptest!` properties (params dropped, `#[test]` added) once the macro's
    // own `#[cfg(test)]` wrapper is gone, so DEH must honour the function's own
    // test attribute, not just module/file cfg.
    let w = detect_in("#[test] fn prop() { a.downcast_ref::<i32>(); }");
    assert!(
        w.is_empty(),
        "a #[test] fn body must be excluded from DEH: {} warning(s)",
        w.len()
    );
}

#[test]
fn downcast_in_cfg_test_fn_excluded() {
    // A function carrying its own `#[cfg(test)]` (not inside a `#[cfg(test)]
    // mod`) is test code too — honour fn-level cfg, mirroring the module case.
    let w = detect_in("#[cfg(test)] fn helper() { a.downcast_ref::<i32>(); }");
    assert!(w.is_empty(), "a #[cfg(test)] fn body must be excluded");
}

#[test]
fn downcast_in_expanded_quickcheck_property_excluded() {
    // End-to-end regression guard: a `#[cfg(test)] quickcheck! { … }` is
    // surfaced by the macro-expansion pre-pass as a synthetic `#[test] fn`
    // with the macro's `#[cfg(test)]` wrapper dropped. DEH must still skip the
    // body because the generated fn carries `#[test]` — otherwise expansion
    // flips a test body into apparent production code.
    use crate::adapters::shared::macro_expansion::expand_test_macros;
    let src =
        "#[cfg(test)] quickcheck! { fn prop(x: u8) -> bool { x.downcast_ref::<u8>().is_some() } }";
    let mut parsed = vec![(
        "lib.rs".to_string(),
        src.to_string(),
        syn::parse_file(src).expect("test source"),
    )];
    expand_test_macros(&mut parsed);
    let config = StructuralConfig::default();
    let cfg_test_files = std::collections::HashSet::new();
    let mut warnings = Vec::new();
    detect_deh(&mut warnings, &parsed, &config, &cfg_test_files);
    assert!(
        warnings.is_empty(),
        "expanded #[test] property body must be excluded from DEH: {warnings:?}"
    );
}

#[test]
fn downcast_in_non_test_module_flagged() {
    // A downcast nested in a regular (non-test) module must be flagged: the
    // visitor has to descend into the module (and keep `in_test = false`).
    // Guards `visit_item_mod` against being turned into a no-op (which would
    // also drop the recursion).
    let w = detect_in("mod inner { fn foo(a: &dyn std::any::Any) { a.downcast_ref::<i32>(); } }");
    assert_eq!(w.len(), 1, "downcast in a non-test module must be flagged");
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
