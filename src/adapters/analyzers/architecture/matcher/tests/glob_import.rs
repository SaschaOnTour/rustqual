use crate::adapters::analyzers::architecture::matcher::find_glob_imports;
use crate::adapters::analyzers::architecture::ViolationKind;

fn parse(src: &str) -> syn::File {
    syn::parse_str(src).expect("test fixture must parse")
}

fn find(src: &str) -> Vec<crate::adapters::analyzers::architecture::MatchLocation> {
    let ast = parse(src);
    find_glob_imports("fixture.rs", &ast)
}

// ── Positive matches ──────────────────────────────────────────────────

#[test]
fn matches_simple_glob_import() {
    let src = r#"use foo::*;"#;
    let hits = find(src);
    assert_eq!(hits.len(), 1, "one glob expected: {hits:?}");
    match &hits[0].kind {
        ViolationKind::GlobImport { base_path } => {
            assert_eq!(base_path, "foo");
        }
        other => panic!("expected GlobImport, got {other:?}"),
    }
}

#[test]
fn matches_nested_glob_import() {
    let src = r#"use foo::bar::baz::*;"#;
    let hits = find(src);
    assert_eq!(hits.len(), 1, "one glob expected: {hits:?}");
    match &hits[0].kind {
        ViolationKind::GlobImport { base_path } => {
            assert_eq!(base_path, "foo::bar::baz");
        }
        other => panic!("expected GlobImport, got {other:?}"),
    }
}

#[test]
fn matches_self_glob() {
    // `use self::*` is still a glob import — policies may accept it via scope,
    // not via the matcher itself.
    let src = r#"use self::*;"#;
    let hits = find(src);
    assert_eq!(hits.len(), 1);
    match &hits[0].kind {
        ViolationKind::GlobImport { base_path } => {
            assert_eq!(base_path, "self");
        }
        other => panic!("expected GlobImport, got {other:?}"),
    }
}

#[test]
fn matches_super_glob() {
    let src = r#"
        mod inner {
            use super::*;
        }
    "#;
    let hits = find(src);
    assert_eq!(hits.len(), 1);
    match &hits[0].kind {
        ViolationKind::GlobImport { base_path } => {
            assert_eq!(base_path, "super");
        }
        other => panic!("expected GlobImport, got {other:?}"),
    }
}

#[test]
fn matches_glob_inside_group() {
    // `use foo::{bar::*, baz}` contains one glob inside the group.
    let src = r#"use foo::{bar::*, baz};"#;
    let hits = find(src);
    assert_eq!(hits.len(), 1, "one glob expected, got: {hits:?}");
    match &hits[0].kind {
        ViolationKind::GlobImport { base_path } => {
            assert_eq!(base_path, "foo::bar");
        }
        other => panic!("expected GlobImport, got {other:?}"),
    }
}

#[test]
fn reports_multiple_globs_separately() {
    let src = r#"
        use foo::*;
        use bar::baz::*;
    "#;
    let hits = find(src);
    assert_eq!(hits.len(), 2, "two globs expected: {hits:?}");
}

// ── Negative matches ──────────────────────────────────────────────────

#[test]
fn does_not_match_named_import() {
    let src = r#"use foo::Bar;"#;
    let hits = find(src);
    assert!(hits.is_empty(), "named import must not match: {hits:?}");
}

#[test]
fn does_not_match_group_without_glob() {
    let src = r#"use foo::{bar, baz};"#;
    let hits = find(src);
    assert!(
        hits.is_empty(),
        "group without glob must not match: {hits:?}"
    );
}

#[test]
fn does_not_match_renamed_import() {
    let src = r#"use foo::Bar as Baz;"#;
    let hits = find(src);
    assert!(hits.is_empty(), "rename is not a glob: {hits:?}");
}

#[test]
fn does_not_match_empty_file() {
    let hits = find("");
    assert!(hits.is_empty());
}

#[test]
fn matches_prelude_glob_at_matcher_level() {
    // The matcher is a dumb "find ALL globs" primitive — it reports prelude
    // globs too (`crate::prelude::*`, `dioxus::prelude::*`, …). The idiomatic
    // exemption is a POLICY decision applied at the analyzer layer via
    // `allow_prelude_glob` (see `run_pattern_matchers`), not here — consistent
    // with the `self::*`/`super::*` handling above.
    for src in [
        "use crate::prelude::*;",
        "use dioxus::prelude::*;",
        "use some::nested::prelude::*;",
    ] {
        let hits = find(src);
        assert_eq!(
            hits.len(),
            1,
            "matcher must report the prelude glob (policy decides exemption), got {hits:?} for `{src}`"
        );
    }
}

#[test]
fn line_number_points_to_glob_statement() {
    let src = "\n\n\nuse foo::*;\n";
    let hits = find(src);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].line, 4, "should point to line of the glob");
}
