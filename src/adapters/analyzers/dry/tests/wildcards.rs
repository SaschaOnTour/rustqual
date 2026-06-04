use crate::adapters::analyzers::dry::wildcards::*;

fn parse(code: &str) -> Vec<(String, String, syn::File)> {
    let syntax = syn::parse_file(code).expect("parse failed");
    vec![("test.rs".to_string(), code.to_string(), syntax)]
}

#[test]
fn test_detects_simple_glob() {
    let parsed = parse("use crate::module::*;");
    let warnings = detect_wildcard_imports(&parsed);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].module_path, "crate::module::*");
    assert_eq!(warnings[0].file, "test.rs");
}

#[test]
fn test_detects_nested_glob() {
    let parsed = parse("use crate::module::sub::*;");
    let warnings = detect_wildcard_imports(&parsed);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].module_path, "crate::module::sub::*");
}

#[test]
fn test_no_warning_for_named_import() {
    let parsed = parse("use crate::module::Foo;");
    let warnings = detect_wildcard_imports(&parsed);
    assert!(warnings.is_empty());
}

#[test]
fn test_no_warning_for_group_import() {
    let parsed = parse("use crate::module::{Foo, Bar};");
    let warnings = detect_wildcard_imports(&parsed);
    assert!(warnings.is_empty());
}

#[test]
fn test_glob_inside_group() {
    let parsed = parse("use crate::{module::*, other::Foo};");
    let warnings = detect_wildcard_imports(&parsed);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].module_path, "crate::module::*");
}

#[test]
fn test_excludes_super_in_test() {
    let code = r#"
        #[cfg(test)]
        mod tests {
            use super::*;
        }
    "#;
    let parsed = parse(code);
    let warnings = detect_wildcard_imports(&parsed);
    assert!(
        warnings.is_empty(),
        "use super::* in #[cfg(test)] should be excluded"
    );
}

#[test]
fn test_super_glob_outside_test_flagged() {
    let code = "use super::*;";
    let parsed = parse(code);
    let warnings = detect_wildcard_imports(&parsed);
    assert_eq!(
        warnings.len(),
        1,
        "use super::* outside test should be flagged"
    );
    assert_eq!(warnings[0].module_path, "super::*");
}

#[test]
fn test_excludes_prelude_glob() {
    let parsed = parse("use std::prelude::*;");
    let warnings = detect_wildcard_imports(&parsed);
    assert!(warnings.is_empty(), "prelude::* should be excluded");
}

#[test]
fn test_excludes_custom_prelude_glob() {
    let parsed = parse("use crate::prelude::*;");
    let warnings = detect_wildcard_imports(&parsed);
    assert!(warnings.is_empty(), "crate::prelude::* should be excluded");
}

#[test]
fn test_external_glob_detected() {
    let parsed = parse("use std::collections::*;");
    let warnings = detect_wildcard_imports(&parsed);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].module_path, "std::collections::*");
}

#[test]
fn wildcard_in_cfg_test_companion_file_skipped() {
    // Wildcard imports in a test file are a common, accepted pattern.
    // A `#![cfg(test)]` companion file (no `tests/` path segment) is test
    // code, so its glob imports must be skipped — recognised via the
    // authoritative cfg-test file set, not a `tests/` path heuristic.
    let code = r#"
        #![cfg(test)]
        use crate::foo::*;
    "#;
    let parsed = vec![(
        "src/bar_tests.rs".to_string(),
        code.to_string(),
        syn::parse_file(code).expect("parse failed"),
    )];
    let warnings = detect_wildcard_imports(&parsed);
    assert!(
        warnings.is_empty(),
        "wildcard import in a #![cfg(test)] file must be skipped: {warnings:?}"
    );
}

#[test]
fn wildcard_in_production_file_under_nested_tests_dir_still_flagged() {
    // A `src/**/tests/` file not reached via `#[cfg(test)] mod` is
    // production code; its wildcard imports must still be flagged.
    let code = "use crate::foo::*;";
    let parsed = vec![(
        "src/database/tests/util.rs".to_string(),
        code.to_string(),
        syn::parse_file(code).expect("parse failed"),
    )];
    let warnings = detect_wildcard_imports(&parsed);
    assert_eq!(
        warnings.len(),
        1,
        "wildcard in a non-cfg-test src/**/tests/ file must be flagged: {warnings:?}"
    );
}

#[test]
fn test_multiple_globs_in_file() {
    let code = "use crate::a::*;\nuse crate::b::*;\nuse crate::c::Foo;";
    let parsed = parse(code);
    let warnings = detect_wildcard_imports(&parsed);
    assert_eq!(warnings.len(), 2);
}

#[test]
fn test_multiple_files() {
    let code1 = "use crate::module::*;";
    let code2 = "use crate::other::*;";
    let syntax1 = syn::parse_file(code1).unwrap();
    let syntax2 = syn::parse_file(code2).unwrap();
    let parsed = vec![
        ("a.rs".to_string(), code1.to_string(), syntax1),
        ("b.rs".to_string(), code2.to_string(), syntax2),
    ];
    let warnings = detect_wildcard_imports(&parsed);
    assert_eq!(warnings.len(), 2);
    assert_eq!(warnings[0].file, "a.rs");
    assert_eq!(warnings[1].file, "b.rs");
}

#[test]
fn test_glob_line_number() {
    let code = "fn foo() {}\nuse crate::module::*;\nfn bar() {}";
    let parsed = parse(code);
    let warnings = detect_wildcard_imports(&parsed);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 2);
}

#[test]
fn test_empty_file() {
    let parsed = parse("");
    let warnings = detect_wildcard_imports(&parsed);
    assert!(warnings.is_empty());
}

#[test]
fn test_not_suppressed_by_default() {
    let parsed = parse("use crate::module::*;");
    let warnings = detect_wildcard_imports(&parsed);
    assert!(!warnings[0].suppressed);
}

#[test]
fn test_pub_use_reexport_excluded() {
    let parsed = parse("pub use crate::module::*;");
    let warnings = detect_wildcard_imports(&parsed);
    assert!(warnings.is_empty(), "pub use re-exports should be excluded");
}

#[test]
fn test_pub_crate_use_reexport_excluded() {
    let parsed = parse("pub(crate) use crate::module::*;");
    let warnings = detect_wildcard_imports(&parsed);
    assert!(
        warnings.is_empty(),
        "pub(crate) use re-exports should be excluded"
    );
}

#[test]
fn test_detects_wildcard_inside_module() {
    // A `use x::*;` nested in an inline module must still be flagged; guards the
    // `visit_item_mod` recursion in `WildcardCollector` against being a no-op.
    let parsed = parse("mod inner { use crate::module::*; }");
    let warnings = detect_wildcard_imports(&parsed);
    assert!(
        !warnings.is_empty(),
        "a wildcard import inside a module must be detected"
    );
}
