//! Snapshot tests against the golden example fixtures in
//! `examples/architecture/<rule>/`.
//!
//! For each rule, the README declares the expected match count and kind.
//! Breaking these tests signals either:
//! - the matcher regressed (a previously caught violation is now missed), or
//! - the rule semantics silently changed (drift without docs update).

use crate::adapters::analyzers::architecture::forbidden_rule::{
    check_forbidden_rules, CompiledForbiddenRule,
};
use crate::adapters::analyzers::architecture::layer_rule::{
    check_layer_rule, LayerDefinitions, LayerRuleInput, UnmatchedBehavior,
};
use crate::adapters::analyzers::architecture::matcher::{
    find_function_call_matches, find_glob_imports, find_item_kind_matches, find_macro_calls,
    find_method_call_matches, find_path_prefix_matches,
};
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Load and parse a Rust source file from a golden example directory.
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

#[test]
fn forbid_method_call_example_matches_direct_and_ufcs() {
    let (file, ast) = load_fixture("forbid_method_call", "src/domain/bad.rs");
    let hits = find_method_call_matches(&file, &ast, &["unwrap".to_string()]);
    assert_eq!(
        hits.len(),
        2,
        "both direct and UFCS forms expected: {hits:?}"
    );
    let syntaxes: Vec<&'static str> = hits
        .iter()
        .filter_map(|h| match &h.kind {
            ViolationKind::MethodCall { syntax, .. } => Some(*syntax),
            _ => None,
        })
        .collect();
    assert!(
        syntaxes.contains(&"direct"),
        "direct form not reported: {syntaxes:?}"
    );
    assert!(
        syntaxes.contains(&"ufcs"),
        "ufcs form not reported: {syntaxes:?}"
    );
}

#[test]
fn forbid_method_call_example_ignores_unrelated_name() {
    let (file, ast) = load_fixture("forbid_method_call", "src/domain/bad.rs");
    let hits = find_method_call_matches(&file, &ast, &["clone".to_string()]);
    assert!(hits.is_empty(), "no clone calls in fixture: {hits:?}");
}

#[test]
fn forbid_macro_call_example_matches_exactly_once() {
    let (file, ast) = load_fixture("forbid_macro_call", "src/domain/bad.rs");
    let hits = find_macro_calls(&file, &ast, &["println".to_string()]);
    let hit = only_hit(hits);
    match &hit.kind {
        ViolationKind::MacroCall { name } => assert_eq!(name, "println"),
        other => panic!("unexpected violation kind: {other:?}"),
    }
    assert_eq!(
        hit.line, 5,
        "println! on line 5 of bad.rs (after header comments)"
    );
}

#[test]
fn forbid_macro_call_example_ignores_unrelated_macros() {
    let (file, ast) = load_fixture("forbid_macro_call", "src/domain/bad.rs");
    let hits = find_macro_calls(&file, &ast, &["panic".to_string()]);
    assert!(hits.is_empty(), "no panic!() in fixture: {hits:?}");
}

#[test]
fn forbid_function_call_example_matches_exactly_once() {
    let (file, ast) = load_fixture("forbid_function_call", "src/domain/bad.rs");
    let hits = find_function_call_matches(&file, &ast, &["Box::new".to_string()]);
    let hit = only_hit(hits);
    match &hit.kind {
        ViolationKind::FunctionCall { rendered_path } => {
            assert_eq!(rendered_path, "Box::new");
        }
        other => panic!("unexpected kind: {other:?}"),
    }
    assert_eq!(
        hit.line, 4,
        "Box::new is on line 4 of bad.rs (after header comments)"
    );
}

#[test]
fn forbid_function_call_example_ignores_unrelated_paths() {
    let (file, ast) = load_fixture("forbid_function_call", "src/domain/bad.rs");
    let hits = find_function_call_matches(&file, &ast, &["Vec::new".to_string()]);
    assert!(hits.is_empty(), "no Vec::new in fixture: {hits:?}");
}

#[test]
fn forbid_item_kind_example_matches_exactly_once() {
    let (file, ast) = load_fixture("forbid_item_kind", "src/domain/bad.rs");
    let hits = find_item_kind_matches(&file, &ast, &["unsafe_fn".to_string()]);
    let hit = only_hit(hits);
    match &hit.kind {
        ViolationKind::ItemKind { kind, name } => {
            assert_eq!(*kind, "unsafe_fn");
            assert_eq!(name, "dangerous");
        }
        other => panic!("unexpected kind: {other:?}"),
    }
    assert_eq!(
        hit.line, 1,
        "unsafe fn is on line 1 of bad.rs (after header comments)"
    );
}

#[test]
fn forbid_item_kind_example_ignores_unrequested_kinds() {
    let (file, ast) = load_fixture("forbid_item_kind", "src/domain/bad.rs");
    let hits = find_item_kind_matches(&file, &ast, &["async_fn".to_string()]);
    assert!(hits.is_empty(), "no async fn in fixture: {hits:?}");
}

// ── Layer Rule example ────────────────────────────────────────────────

fn example_dir(example: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("architecture")
        .join(example)
}

fn load_workspace(example: &str) -> Vec<(String, syn::File)> {
    let root = example_dir(example);
    walkdir::WalkDir::new(root.join("src"))
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
        .map(|entry| load_one(&root, entry.path()))
        .collect()
}

fn load_one(root: &Path, path: &Path) -> (String, syn::File) {
    let source = fs::read_to_string(path).expect("read fixture");
    let ast: syn::File = syn::parse_str(&source).expect("parse fixture");
    let rel = path
        .strip_prefix(root)
        .expect("strip prefix")
        .to_string_lossy()
        .replace('\\', "/");
    (rel, ast)
}

fn glob_set(patterns: &[&str]) -> GlobSet {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).expect("valid glob"));
    }
    b.build().expect("valid glob set")
}

#[test]
fn layer_example_produces_exactly_one_violation() {
    let workspace = load_workspace("layer");
    let refs: Vec<(String, &syn::File)> = workspace.iter().map(|(p, f)| (p.clone(), f)).collect();
    let layers = LayerDefinitions::new(
        vec!["domain".to_string(), "adapter".to_string()],
        vec![
            ("domain".to_string(), glob_set(&["src/domain/**"])),
            ("adapter".to_string(), glob_set(&["src/adapters/**"])),
        ],
    );
    let hits = check_layer_rule(
        &refs,
        &LayerRuleInput {
            layers: &layers,
            reexport_points: &glob_set(&[]),
            unmatched_behavior: UnmatchedBehavior::CompositionRoot,
            external_exact: &HashMap::new(),
            external_glob: &[],
        },
    );
    assert_eq!(hits.len(), 1, "expected exactly one layer hit: {hits:?}");
    match &hits[0].kind {
        ViolationKind::LayerViolation {
            from_layer,
            to_layer,
            ..
        } => {
            assert_eq!(from_layer, "domain");
            assert_eq!(to_layer, "adapter");
        }
        other => panic!("unexpected kind: {other:?}"),
    }
    assert!(
        hits[0].file.ends_with("src/domain/bad.rs"),
        "file = {}",
        hits[0].file
    );
}

// ── Forbidden Rule example ────────────────────────────────────────────

#[test]
fn forbidden_example_produces_exactly_one_violation() {
    let workspace = load_workspace("forbidden");
    let refs: Vec<(String, &syn::File)> = workspace.iter().map(|(p, f)| (p.clone(), f)).collect();
    let rule = CompiledForbiddenRule {
        from: Glob::new("src/adapters/analyzers/iosp/**")
            .unwrap()
            .compile_matcher(),
        to: Glob::new("src/adapters/analyzers/**")
            .unwrap()
            .compile_matcher(),
        except: glob_set(&["src/adapters/analyzers/iosp/**"]),
        reason: "peer analyzers are isolated".to_string(),
    };
    let hits = check_forbidden_rules(&refs, std::slice::from_ref(&rule));
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one forbidden hit: {hits:?}"
    );
    match &hits[0].kind {
        ViolationKind::ForbiddenEdge {
            reason,
            imported_path,
        } => {
            assert_eq!(reason, "peer analyzers are isolated");
            assert!(
                imported_path.starts_with("crate::adapters::analyzers::srp"),
                "imported_path = {imported_path:?}"
            );
        }
        other => panic!("unexpected kind: {other:?}"),
    }
    assert!(
        hits[0].file.ends_with("iosp/bad.rs"),
        "file = {}",
        hits[0].file
    );
}
