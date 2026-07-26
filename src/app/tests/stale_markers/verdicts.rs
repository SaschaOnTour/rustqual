//! Verdicts for markers that ARE attached to a function: spent (production
//! calls it now) and never-applied (the item is crate-internal).

use super::*;

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
