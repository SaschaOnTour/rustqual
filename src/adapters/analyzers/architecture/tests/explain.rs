//! Unit tests for the `--explain` diagnostic renderer.
//!
//! Each test builds a minimal `CompiledArchitecture`, parses a fixture file,
//! and asserts that `render_explain` returns text containing the expected
//! structural markers. Formatting details (spaces, punctuation) are not
//! asserted to keep the tests resilient to cosmetic tweaks.

use crate::adapters::analyzers::architecture::compiled::CompiledArchitecture;
use crate::adapters::analyzers::architecture::explain::{explain_file, ImportKind};
use crate::adapters::analyzers::architecture::forbidden_rule::CompiledForbiddenRule;
use crate::adapters::analyzers::architecture::layer_rule::{LayerDefinitions, UnmatchedBehavior};
use globset::{Glob, GlobMatcher, GlobSet, GlobSetBuilder};
use std::collections::HashMap;

fn matcher(pattern: &str) -> GlobMatcher {
    Glob::new(pattern).expect("valid glob").compile_matcher()
}

fn globset(patterns: &[&str]) -> GlobSet {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).expect("valid glob"));
    }
    b.build().expect("valid glob set")
}

fn minimal_compiled() -> CompiledArchitecture {
    CompiledArchitecture {
        layers: LayerDefinitions::new(
            vec![
                "domain".to_string(),
                "port".to_string(),
                "application".to_string(),
                "adapter".to_string(),
            ],
            vec![
                ("domain".to_string(), globset(&["src/domain/**"])),
                ("port".to_string(), globset(&["src/ports/**"])),
                ("application".to_string(), globset(&["src/app/**"])),
                ("adapter".to_string(), globset(&["src/adapters/**"])),
            ],
        ),
        reexport_points: globset(&["src/lib.rs"]),
        unmatched_behavior: UnmatchedBehavior::CompositionRoot,
        external_exact: HashMap::new(),
        external_glob: Vec::new(),
        forbidden: Vec::new(),
    }
}

fn parse_file(src: &str) -> syn::File {
    syn::parse_str(src).expect("parse")
}

// ── file layer classification ──────────────────────────────────────────

#[test]
fn reports_assigned_layer() {
    let ast = parse_file("fn f() {}");
    let report = explain_file("src/domain/foo.rs", &ast, &minimal_compiled());
    assert_eq!(report.layer.as_deref(), Some("domain"));
    assert_eq!(report.rank, Some(0));
    assert!(!report.is_reexport);
}

#[test]
fn reports_reexport_point() {
    let ast = parse_file("pub use crate::adapters::X;");
    let report = explain_file("src/lib.rs", &ast, &minimal_compiled());
    assert!(report.is_reexport);
    assert!(report.layer_violations.is_empty());
    assert!(report.forbidden_violations.is_empty());
}

#[test]
fn reports_unmatched_file() {
    let ast = parse_file("fn f() {}");
    let report = explain_file("src/misc/foo.rs", &ast, &minimal_compiled());
    assert!(report.layer.is_none());
    assert!(!report.is_reexport);
}

// ── import classification ──────────────────────────────────────────────

#[test]
fn classifies_crate_import_resolved() {
    let ast = parse_file("use crate::adapters::foo::Bar;");
    let report = explain_file("src/adapters/mod.rs", &ast, &minimal_compiled());
    let imports = &report.imports;
    assert_eq!(imports.len(), 1);
    match &imports[0].kind {
        ImportKind::CrateInternal { target_layer, .. } => {
            assert_eq!(target_layer.as_deref(), Some("adapter"));
        }
        other => panic!("expected CrateInternal, got {other:?}"),
    }
}

#[test]
fn classifies_stdlib_imports_as_ignored() {
    let ast = parse_file("use std::collections::HashMap; use core::fmt;");
    let report = explain_file("src/domain/foo.rs", &ast, &minimal_compiled());
    assert!(report
        .imports
        .iter()
        .all(|i| matches!(i.kind, ImportKind::Ignored { .. })));
}

#[test]
fn classifies_external_exact() {
    let mut compiled = minimal_compiled();
    compiled
        .external_exact
        .insert("tokio".to_string(), "adapter".to_string());
    let ast = parse_file("use tokio::spawn;");
    let report = explain_file("src/adapters/net.rs", &ast, &compiled);
    match &report.imports[0].kind {
        ImportKind::ExternalCrate {
            crate_name,
            resolved_layer,
            ..
        } => {
            assert_eq!(crate_name, "tokio");
            assert_eq!(resolved_layer.as_deref(), Some("adapter"));
        }
        other => panic!("expected ExternalCrate, got {other:?}"),
    }
}

#[test]
fn classifies_external_unknown_as_no_mapping() {
    let ast = parse_file("use mystery_crate::Foo;");
    let report = explain_file("src/adapters/x.rs", &ast, &minimal_compiled());
    match &report.imports[0].kind {
        ImportKind::ExternalCrate { resolved_layer, .. } => assert!(resolved_layer.is_none()),
        other => panic!("unexpected kind: {other:?}"),
    }
}

// ── violations ─────────────────────────────────────────────────────────

#[test]
fn layer_violation_surfaced() {
    let ast = parse_file("use crate::adapters::X;");
    let report = explain_file("src/domain/bad.rs", &ast, &minimal_compiled());
    assert_eq!(
        report.layer_violations.len(),
        1,
        "{:?}",
        report.layer_violations
    );
}

#[test]
fn forbidden_violation_surfaced() {
    let mut compiled = minimal_compiled();
    compiled.forbidden.push(CompiledForbiddenRule {
        from: matcher("src/domain/**"),
        to: matcher("src/adapters/**"),
        except: globset(&[]),
        reason: "no outward imports".to_string(),
    });
    let ast = parse_file("use crate::adapters::X;");
    let report = explain_file("src/domain/bad.rs", &ast, &compiled);
    assert_eq!(report.forbidden_violations.len(), 1);
}

// ── text rendering ─────────────────────────────────────────────────────

#[test]
fn render_mentions_layer_name() {
    let ast = parse_file("fn f() {}");
    let report = explain_file("src/domain/foo.rs", &ast, &minimal_compiled());
    let text = report.render();
    assert!(text.contains("domain"), "{text}");
    assert!(text.contains("src/domain/foo.rs"), "{text}");
}

#[test]
fn render_marks_reexport_point() {
    let ast = parse_file("fn f() {}");
    let report = explain_file("src/lib.rs", &ast, &minimal_compiled());
    let text = report.render();
    assert!(text.to_lowercase().contains("re-export"), "{text}");
}

#[test]
fn render_includes_violation_sections() {
    let mut compiled = minimal_compiled();
    compiled.forbidden.push(CompiledForbiddenRule {
        from: matcher("src/domain/**"),
        to: matcher("src/adapters/**"),
        except: globset(&[]),
        reason: "no outward imports".to_string(),
    });
    let ast = parse_file("use crate::adapters::X;");
    let report = explain_file("src/domain/bad.rs", &ast, &compiled);
    let text = report.render();
    assert!(text.to_lowercase().contains("layer violation"), "{text}");
    assert!(text.to_lowercase().contains("forbidden"), "{text}");
}
