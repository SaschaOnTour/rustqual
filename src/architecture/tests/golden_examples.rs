//! Snapshot tests against the golden example fixtures in
//! `examples/architecture/<rule>/`.
//!
//! For each rule, the README declares the expected match count and kind.
//! Breaking these tests signals either:
//! - the matcher regressed (a previously caught violation is now missed), or
//! - the rule semantics silently changed (drift without docs update).

use crate::architecture::matcher::{find_glob_imports, find_path_prefix_matches};
use crate::architecture::{MatchLocation, ViolationKind};
use std::fs;
use std::path::Path;

/// Load and parse a Rust source file from a golden example directory.
#[cfg(test)]
fn load_fixture(example: &str, rel: &str) -> (String, syn::File) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("architecture")
        .join(example)
        .join(rel);
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let ast: syn::File = syn::parse_str(&source)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));
    (path.display().to_string(), ast)
}

#[cfg(test)]
fn only_hit(hits: Vec<MatchLocation>) -> MatchLocation {
    assert_eq!(
        hits.len(),
        1,
        "exactly one violation expected from the golden fixture: {hits:?}"
    );
    hits.into_iter().next().unwrap()
}

#[test]
fn forbid_path_prefix_example_matches_exactly_once() {
    let (file, ast) = load_fixture("forbid_path_prefix", "src/domain/bad.rs");
    let hits = find_path_prefix_matches(&file, &ast, &["tokio::".to_string()]);
    let hit = only_hit(hits);
    match &hit.kind {
        ViolationKind::PathPrefix {
            prefix,
            rendered_path,
        } => {
            assert_eq!(prefix, "tokio::");
            assert_eq!(rendered_path, "tokio::spawn");
        }
        other => panic!("unexpected violation kind: {other:?}"),
    }
    assert_eq!(
        hit.line, 4,
        "use statement is on line 4 of bad.rs (after header comments)"
    );
}

#[test]
fn forbid_glob_import_example_matches_exactly_once() {
    let (file, ast) = load_fixture("forbid_glob_import", "src/domain/bad.rs");
    let hits = find_glob_imports(&file, &ast);
    let hit = only_hit(hits);
    match &hit.kind {
        ViolationKind::GlobImport { base_path } => {
            assert_eq!(base_path, "some_crate");
        }
        other => panic!("unexpected violation kind: {other:?}"),
    }
    assert_eq!(
        hit.line, 4,
        "glob import is on line 4 of bad.rs (after header comments)"
    );
}

#[test]
fn forbid_path_prefix_example_has_no_hits_for_unrelated_prefix() {
    let (file, ast) = load_fixture("forbid_path_prefix", "src/domain/bad.rs");
    let hits = find_path_prefix_matches(&file, &ast, &["anyhow::".to_string()]);
    assert!(
        hits.is_empty(),
        "unrelated prefix must not produce hits: {hits:?}"
    );
}

#[test]
fn forbid_glob_import_example_counted_as_path_prefix_on_base() {
    // The glob `use some_crate::*;` renders its tail as `some_crate::*`
    // and matches a prefix `some_crate::`. Useful to cross-check that the
    // two matchers cooperate without double-reporting the same line.
    let (file, ast) = load_fixture("forbid_glob_import", "src/domain/bad.rs");
    let hits = find_path_prefix_matches(&file, &ast, &["some_crate::".to_string()]);
    assert_eq!(
        hits.len(),
        1,
        "the glob target renders and matches the prefix exactly once: {hits:?}"
    );
}
