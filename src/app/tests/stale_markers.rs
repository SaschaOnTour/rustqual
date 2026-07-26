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

use std::collections::{HashMap, HashSet};

use crate::adapters::shared::declared_function::DeclaredFunction;
use crate::adapters::shared::reachability::{compute_external_reach, ExternalReach};
use crate::app::stale_markers::detect_stale_marker_orphans;
use crate::domain::findings::{MarkerKind, OrphanSuppression};

/// A declared production fn at `line` carrying the given marker.
fn declared(name: &str, line: usize, api: bool, helper: bool) -> DeclaredFunction {
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

fn names(items: &[&str]) -> HashSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// Marker lines for `src/lib.rs`.
fn marker_lines(lines: &[usize]) -> HashMap<String, HashSet<usize>> {
    let mut m = HashMap::new();
    m.insert("src/lib.rs".to_string(), lines.iter().copied().collect());
    m
}

/// Marker lines for one arbitrary file.
fn lines_in(file: &str, line: usize) -> HashMap<String, HashSet<usize>> {
    let mut m = HashMap::new();
    m.insert(file.to_string(), [line].into_iter().collect());
    m
}

/// Reachability derived from real sources, so the tests exercise the same
/// derivation production uses.
fn reach_of(files: &[(&str, &str)]) -> ExternalReach {
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
fn reach_public(fns: &[&str]) -> ExternalReach {
    let src = fns
        .iter()
        .map(|n| format!("pub fn {n}() {{}}"))
        .collect::<Vec<_>>()
        .join("\n");
    reach_of(&[("src/lib.rs", src.as_str())])
}

/// A private `mod guts;` holding `pub fn <name>` — unreachable from outside.
fn reach_internal(name: &str) -> ExternalReach {
    let src = format!("pub fn {name}() {{}}");
    reach_of(&[("src/lib.rs", "mod guts;"), ("src/guts.rs", src.as_str())])
}

/// One `qual:api`-marked, externally reachable fn `f` at line 10, marker on 9.
fn api_orphans(prod: &[&str]) -> Vec<OrphanSuppression> {
    detect_stale_marker_orphans(
        &[declared("f", 10, true, false)],
        &names(prod),
        &marker_lines(&[9]),
        &HashMap::new(),
        &reach_public(&["f"]),
    )
}

/// One marked fn living in the private `src/guts.rs`.
fn internal_orphans(prod: &[&str], api: bool) -> Vec<OrphanSuppression> {
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

// ── spent markers (the premise was true, then production caught up) ──────

#[test]
fn api_marker_is_spent_once_production_calls_the_function() {
    let out = api_orphans(&["f"]);
    assert_eq!(out.len(), 1, "prod-called ⇒ marker is spent: {out:?}");
    assert_eq!(out[0].marker, MarkerKind::Api);
    assert_eq!(out[0].line, 9, "must point at the marker, not the fn");
    assert_eq!(out[0].file, "src/lib.rs");
}

#[test]
fn api_marker_on_real_public_api_without_callers_stays_silent() {
    // The one legitimate state: reachable from outside, nothing in-crate calls
    // it. Reporting it would push the author to delete the marker that is
    // holding back a real DRY-002 finding.
    assert!(
        api_orphans(&[]).is_empty(),
        "reachable + uncalled ⇒ marker still does its job"
    );
}

#[test]
fn spent_marker_message_warns_that_untested_may_surface() {
    let reason = api_orphans(&["f"])[0].reason.clone().unwrap_or_default();
    assert!(
        reason.contains("production calls") && reason.contains("untested"),
        "spent message must name cause and consequence: {reason}"
    );
}

#[test]
fn qualified_name_counts_as_a_production_caller() {
    // Call graphs record methods by qualified name (`Type::method`); matching
    // only the bare name would keep a spent marker on a method invisible.
    let mut d = declared("as_slice", 10, true, false);
    d.qualified_name = "Embedding::as_slice".to_string();
    let out = detect_stale_marker_orphans(
        &[d],
        &names(&["Embedding::as_slice"]),
        &marker_lines(&[9]),
        &HashMap::new(),
        &reach_public(&["as_slice"]),
    );
    assert_eq!(out.len(), 1, "qualified-name match must count: {out:?}");
}

// ── markers that never applied (crate-internal items) ────────────────────

#[test]
fn api_marker_on_unreachable_called_item_says_remove_it() {
    // rustqual's own shape: `mod guts;` is private, so nothing outside can
    // call `f` — and production already does.
    let out = internal_orphans(&["f"], true);
    assert_eq!(out.len(), 1, "unreachable ⇒ the marker never applied");
    let reason = out[0].reason.clone().unwrap_or_default();
    assert!(
        reason.contains("outside the crate") && reason.contains("remove the marker"),
        "must explain why it never applied and what to do: {reason}"
    );
}

#[test]
fn api_marker_on_unreachable_uncalled_item_says_wire_or_delete() {
    let out = internal_orphans(&[], true);
    assert_eq!(out.len(), 1);
    let reason = out[0].reason.clone().unwrap_or_default();
    assert!(
        reason.contains("outside the crate") && reason.contains("delete it"),
        "must offer the wire-or-delete remedy: {reason}"
    );
    assert!(
        reason.contains("dead-code"),
        "must warn that dead code surfaces once removed: {reason}"
    );
}

// ── qual:test_helper ─────────────────────────────────────────────────────

#[test]
fn test_helper_marker_is_spent_when_production_calls_it() {
    let out = detect_stale_marker_orphans(
        &[declared("h", 10, false, true)],
        &names(&["h"]),
        &HashMap::new(),
        &marker_lines(&[9]),
        &reach_public(&["h"]),
    );
    assert_eq!(out.len(), 1, "a prod-called helper is no longer test-only");
    assert_eq!(out[0].marker, MarkerKind::TestHelper);
    assert!(
        out[0]
            .reason
            .clone()
            .unwrap_or_default()
            .contains("qual:test_helper"),
        "message must name the marker it is about"
    );
}

#[test]
fn test_helper_marker_is_not_judged_by_reachability() {
    // Being unreachable from outside is a helper's normal state — that is the
    // whole point of the marker, so it must never be reported for it.
    assert!(
        internal_orphans(&[], false).is_empty(),
        "internal is normal for a helper"
    );
}

#[test]
fn test_helper_without_any_caller_is_left_to_dead_code() {
    // `qual:test_helper` deliberately does not suppress the `uncalled`
    // variant, so a helper nobody calls already surfaces as DRY-002.
    // Reporting it here too would double-report one defect.
    let out = detect_stale_marker_orphans(
        &[declared("h", 10, false, true)],
        &names(&[]),
        &HashMap::new(),
        &marker_lines(&[9]),
        &reach_public(&["h"]),
    );
    assert!(out.is_empty(), "no callers ⇒ DRY-002 reports it: {out:?}");
}

// ── general behaviour ────────────────────────────────────────────────────

#[test]
fn unmarked_function_is_never_reported() {
    let out = detect_stale_marker_orphans(
        &[declared("plain", 10, false, false)],
        &names(&["plain"]),
        &HashMap::new(),
        &HashMap::new(),
        &reach_public(&["plain"]),
    );
    assert!(out.is_empty(), "no marker ⇒ nothing to report: {out:?}");
}

#[test]
fn marker_without_a_comment_line_in_range_is_skipped() {
    // Defensive: the declaration says it is marked, but no marker line sits in
    // the annotation window. Better silent than a finding at a made-up line.
    let out = detect_stale_marker_orphans(
        &[declared("f", 40, true, false)],
        &names(&["f"]),
        &marker_lines(&[9]),
        &HashMap::new(),
        &reach_public(&["f"]),
    );
    assert!(out.is_empty(), "no marker line in window ⇒ skip: {out:?}");
}

#[test]
fn findings_are_sorted_by_file_then_line() {
    let mut first = declared("a", 10, true, false);
    first.file = "src/b.rs".to_string();
    let mut second = declared("b", 30, true, false);
    second.file = "src/a.rs".to_string();
    let mut lines = HashMap::new();
    lines.insert("src/b.rs".to_string(), [9].into_iter().collect());
    lines.insert("src/a.rs".to_string(), [29].into_iter().collect());
    let reach = reach_of(&[
        ("src/lib.rs", "pub mod a; pub mod b;"),
        ("src/a.rs", "pub fn b() {}"),
        ("src/b.rs", "pub fn a() {}"),
    ]);
    let out = detect_stale_marker_orphans(
        &[first, second],
        &names(&["a", "b"]),
        &lines,
        &HashMap::new(),
        &reach,
    );
    let order: Vec<&str> = out.iter().map(|o| o.file.as_str()).collect();
    assert_eq!(order, vec!["src/a.rs", "src/b.rs"], "stable ordering");
}
