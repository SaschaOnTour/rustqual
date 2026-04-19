//! Unit tests for the Forbidden rule.
//!
//! Covers the four axes of `[[architecture.forbidden]]`:
//!   1. `from` glob filters which files are subject to the rule.
//!   2. `to` glob matches candidate target paths derived from imports.
//!   3. `except` escape hatch suppresses hits for whitelisted targets.
//!   4. Multiple rules evaluated independently; external-crate imports are
//!      not affected (no path-like target).

use crate::adapters::analyzers::architecture::forbidden_rule::{
    check_forbidden_rules, CompiledForbiddenRule,
};
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use globset::{Glob, GlobMatcher, GlobSet, GlobSetBuilder};

// ── helpers ────────────────────────────────────────────────────────────

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

fn parse_file(src: &str) -> syn::File {
    syn::parse_str(src).expect("test fixture must parse")
}

struct Fixture {
    parsed: Vec<(String, syn::File)>,
}

impl Fixture {
    fn new(files: &[(&str, &str)]) -> Self {
        Self {
            parsed: files
                .iter()
                .map(|(p, s)| (p.to_string(), parse_file(s)))
                .collect(),
        }
    }

    fn refs(&self) -> Vec<(String, &syn::File)> {
        self.parsed.iter().map(|(p, f)| (p.clone(), f)).collect()
    }
}

fn rule(from: &str, to: &str, reason: &str) -> CompiledForbiddenRule {
    CompiledForbiddenRule {
        from: matcher(from),
        to: matcher(to),
        except: globset(&[]),
        reason: reason.to_string(),
    }
}

fn rule_with_except(from: &str, to: &str, except: &[&str], reason: &str) -> CompiledForbiddenRule {
    CompiledForbiddenRule {
        from: matcher(from),
        to: matcher(to),
        except: globset(except),
        reason: reason.to_string(),
    }
}

fn run(fx: &Fixture, rules: &[CompiledForbiddenRule]) -> Vec<MatchLocation> {
    let refs = fx.refs();
    check_forbidden_rules(&refs, rules)
}

// ── basic cases ────────────────────────────────────────────────────────

#[test]
fn clean_file_no_violations() {
    let fx = Fixture::new(&[("src/domain/foo.rs", "pub struct Foo;")]);
    let rules = vec![rule(
        "src/adapters/analyzers/iosp/**",
        "src/adapters/analyzers/*/**",
        "peers isolated",
    )];
    assert!(run(&fx, &rules).is_empty());
}

#[test]
fn file_not_matching_from_is_skipped() {
    // Same import, but file is outside `from`.
    let fx = Fixture::new(&[(
        "src/domain/foo.rs",
        "use crate::adapters::analyzers::srp::X;",
    )]);
    let rules = vec![rule(
        "src/adapters/analyzers/iosp/**",
        "src/adapters/analyzers/srp/**",
        "peers isolated",
    )];
    assert!(run(&fx, &rules).is_empty());
}

#[test]
fn from_matching_file_with_to_matching_import_flagged() {
    let fx = Fixture::new(&[(
        "src/adapters/analyzers/iosp/mod.rs",
        "use crate::adapters::analyzers::srp::Something;",
    )]);
    let rules = vec![rule(
        "src/adapters/analyzers/iosp/**",
        "src/adapters/analyzers/srp/**",
        "peers isolated",
    )];
    let hits = run(&fx, &rules);
    assert_eq!(hits.len(), 1, "{hits:?}");
    match &hits[0].kind {
        ViolationKind::ForbiddenEdge {
            reason,
            imported_path,
        } => {
            assert_eq!(reason, "peers isolated");
            assert!(imported_path.starts_with("crate::adapters::analyzers::srp"));
        }
        other => panic!("unexpected kind: {other:?}"),
    }
    assert_eq!(hits[0].file, "src/adapters/analyzers/iosp/mod.rs");
}

#[test]
fn import_of_different_module_same_adapter_tree_ok_when_to_is_peer_only() {
    // iosp importing from its own tree is fine.
    let fx = Fixture::new(&[(
        "src/adapters/analyzers/iosp/mod.rs",
        "use crate::adapters::analyzers::iosp::scope::ProjectScope;",
    )]);
    // `to` only matches analyzers/<something> but must NOT include iosp itself.
    // Use an except to exclude iosp.
    let rules = vec![rule_with_except(
        "src/adapters/analyzers/iosp/**",
        "src/adapters/analyzers/**",
        &["src/adapters/analyzers/iosp/**"],
        "isolate from peers",
    )];
    assert!(run(&fx, &rules).is_empty());
}

// ── except handling ────────────────────────────────────────────────────

#[test]
fn except_suppresses_specific_targets() {
    // domain must not import from anywhere EXCEPT src/shared/**.
    let fx = Fixture::new(&[
        ("src/domain/a.rs", "use crate::shared::util::X;"),
        ("src/domain/b.rs", "use crate::adapters::mod_::Y;"),
    ]);
    let rules = vec![rule_with_except(
        "src/domain/**",
        "src/**",
        &["src/domain/**", "src/shared/**"],
        "domain isolated",
    )];
    let hits = run(&fx, &rules);
    assert_eq!(hits.len(), 1, "only adapters import flagged: {hits:?}");
    assert_eq!(hits[0].file, "src/domain/b.rs");
}

#[test]
fn except_matching_any_candidate_suppresses_hit() {
    // If ANY candidate path of the import matches except, suppress.
    let fx = Fixture::new(&[(
        "src/adapters/analyzers/iosp/mod.rs",
        "use crate::adapters::analyzers::iosp::types::Foo;",
    )]);
    let rules = vec![rule_with_except(
        "src/adapters/analyzers/iosp/**",
        "src/adapters/analyzers/**",
        &["src/adapters/analyzers/iosp/**"],
        "isolate peers",
    )];
    assert!(run(&fx, &rules).is_empty());
}

// ── resolution candidates ──────────────────────────────────────────────

#[test]
fn import_matches_leaf_module_file_candidate() {
    // `crate::a::b::c` — candidate `src/a/b/c.rs` should match `to = src/a/b/**`.
    let fx = Fixture::new(&[(
        "src/domain/x.rs",
        "use crate::adapters::analyzers::report::print;",
    )]);
    let rules = vec![rule(
        "src/domain/**",
        "src/adapters/analyzers/report/**",
        "domain → report forbidden",
    )];
    let hits = run(&fx, &rules);
    assert_eq!(hits.len(), 1, "{hits:?}");
}

#[test]
fn import_matches_module_dir_mod_rs_candidate() {
    // `crate::a::b` — candidate `src/a/b/mod.rs` should match `to = src/a/b/**`.
    let fx = Fixture::new(&[("src/domain/x.rs", "use crate::adapters::report;")]);
    let rules = vec![rule(
        "src/domain/**",
        "src/adapters/report/**",
        "domain → report forbidden",
    )];
    assert_eq!(run(&fx, &rules).len(), 1);
}

// ── external crates ────────────────────────────────────────────────────

#[test]
fn external_crate_imports_not_affected() {
    // `tokio::spawn` has no crate-internal path; Forbidden rule is path-based
    // and should ignore it (path prefix matchers exist separately).
    let fx = Fixture::new(&[(
        "src/adapters/analyzers/iosp/mod.rs",
        "use tokio::spawn; use serde::Deserialize;",
    )]);
    let rules = vec![rule(
        "src/adapters/analyzers/iosp/**",
        "**",
        "nothing imported",
    )];
    assert!(run(&fx, &rules).is_empty());
}

#[test]
fn self_super_std_ignored() {
    let fx = Fixture::new(&[(
        "src/adapters/analyzers/iosp/mod.rs",
        "use self::inner; use super::parent; use std::io;",
    )]);
    let rules = vec![rule(
        "src/adapters/analyzers/iosp/**",
        "**",
        "none should match",
    )];
    assert!(run(&fx, &rules).is_empty());
}

// ── multiple rules ─────────────────────────────────────────────────────

#[test]
fn multiple_rules_evaluated_independently() {
    let fx = Fixture::new(&[(
        "src/adapters/analyzers/iosp/mod.rs",
        "use crate::adapters::analyzers::srp::X; use crate::adapters::report::Y;",
    )]);
    let rules = vec![
        rule(
            "src/adapters/analyzers/iosp/**",
            "src/adapters/analyzers/srp/**",
            "peers",
        ),
        rule(
            "src/adapters/analyzers/iosp/**",
            "src/adapters/report/**",
            "no reports",
        ),
    ];
    let hits = run(&fx, &rules);
    assert_eq!(hits.len(), 2, "one hit per rule: {hits:?}");
    let reasons: Vec<&str> = hits
        .iter()
        .filter_map(|h| match &h.kind {
            ViolationKind::ForbiddenEdge { reason, .. } => Some(reason.as_str()),
            _ => None,
        })
        .collect();
    assert!(reasons.contains(&"peers"));
    assert!(reasons.contains(&"no reports"));
}

// ── grouped imports ────────────────────────────────────────────────────

#[test]
fn grouped_use_flags_each_matching_leaf() {
    let fx = Fixture::new(&[(
        "src/adapters/analyzers/iosp/mod.rs",
        "use crate::{adapters::analyzers::srp::X, domain::Y};",
    )]);
    let rules = vec![rule(
        "src/adapters/analyzers/iosp/**",
        "src/adapters/analyzers/srp/**",
        "peers",
    )];
    assert_eq!(run(&fx, &rules).len(), 1);
}
