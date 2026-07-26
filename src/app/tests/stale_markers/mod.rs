//! Stale `qual:api` / `qual:test_helper` detection.
//!
//! Three things can be wrong with a `qual:api`, and each has its own remedy:
//! - it **never applied** — the item cannot be named from outside the crate;
//! - it applied once but is **spent** — production calls the function now;
//! - nothing is wrong — real public API with no in-crate callers (silent).
//!
//! The rule deliberately does not also require the function to be tested.
//! TQ-003 only fires for functions that already have production callers, so
//! for a genuine outside-the-crate entry point the TQ-003 exclusion never
//! applies anyway; requiring "and tested" would only let a spent marker keep
//! hiding a real TQ-003 finding.

pub(super) use std::collections::{HashMap, HashSet};

pub(super) use crate::adapters::shared::declared_function::DeclaredFunction;
use crate::adapters::shared::reachability::{compute_external_reach, ExternalReach};
pub(super) use crate::app::stale_markers::detect_stale_marker_orphans;
pub(super) use crate::domain::findings::{MarkerKind, OrphanSuppression};

/// A declared production fn at `line` carrying the given marker.
pub(super) fn declared(name: &str, line: usize, api: bool, helper: bool) -> DeclaredFunction {
    DeclaredFunction {
        name: name.to_string(),
        qualified_name: name.to_string(),
        file: "src/lib.rs".to_string(),
        line,
        is_test: false,
        is_main: false,
        is_trait_impl: false,
        has_allow_dead_code: false,
        is_api: api,
        is_test_helper: helper,
    }
}

pub(super) fn names(items: &[&str]) -> HashSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// Marker lines for `src/lib.rs`.
pub(super) fn marker_lines(lines: &[usize]) -> HashMap<String, HashSet<usize>> {
    let mut m = HashMap::new();
    m.insert("src/lib.rs".to_string(), lines.iter().copied().collect());
    m
}

/// Marker lines for one arbitrary file.
pub(super) fn lines_in(file: &str, line: usize) -> HashMap<String, HashSet<usize>> {
    let mut m = HashMap::new();
    m.insert(file.to_string(), [line].into_iter().collect());
    m
}

/// Reachability derived from real sources, so the tests exercise the same
/// derivation production uses.
pub(super) fn reach_of(files: &[(&str, &str)]) -> ExternalReach {
    let parsed: Vec<(String, String, syn::File)> = files
        .iter()
        .map(|(p, s)| {
            (
                p.to_string(),
                s.to_string(),
                syn::parse_file(s).expect("fixture parses"),
            )
        })
        .collect();
    compute_external_reach(&parsed)
}

/// A crate root where every named fn is genuine public API.
pub(super) fn reach_public(fns: &[&str]) -> ExternalReach {
    let src = fns
        .iter()
        .map(|n| format!("pub fn {n}() {{}}"))
        .collect::<Vec<_>>()
        .join("\n");
    reach_of(&[("src/lib.rs", src.as_str())])
}

/// A private `mod guts;` holding `pub fn <name>` — unreachable from outside.
pub(super) fn reach_internal(name: &str) -> ExternalReach {
    let src = format!("pub fn {name}() {{}}");
    reach_of(&[("src/lib.rs", "mod guts;"), ("src/guts.rs", src.as_str())])
}

/// One `qual:api`-marked, externally reachable fn `f` at line 10, marker on 9.
pub(super) fn api_orphans(prod: &[&str]) -> Vec<OrphanSuppression> {
    detect_stale_marker_orphans(
        &[declared("f", 10, true, false)],
        &names(prod),
        &marker_lines(&[9]),
        &HashMap::new(),
        &reach_public(&["f"]),
    )
}

/// One marked fn living in the private `src/guts.rs`.
pub(super) fn internal_orphans(prod: &[&str], api: bool) -> Vec<OrphanSuppression> {
    let mut d = declared("f", 10, api, !api);
    d.file = "src/guts.rs".to_string();
    let lines = lines_in("src/guts.rs", 9);
    let (api_lines, helper_lines) = if api {
        (lines, HashMap::new())
    } else {
        (HashMap::new(), lines)
    };
    detect_stale_marker_orphans(
        &[d],
        &names(prod),
        &api_lines,
        &helper_lines,
        &reach_internal("f"),
    )
}

mod ineffective;
mod verdicts;
