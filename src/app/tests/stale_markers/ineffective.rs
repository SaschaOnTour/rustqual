//! Markers that cannot affect anything: attached to no function at all, or
//! attached to a function both checks already exempt.

use super::*;

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
fn marker_out_of_range_of_its_function_is_reported_as_unattached() {
    // The marker sits far above the function, so `mark_api_declarations`
    // never applied it — which makes it unattached, not silent. The finding
    // is anchored at the marker's own line, never at a made-up one.
    let out = detect_stale_marker_orphans(
        &[declared("f", 40, true, false)],
        &names(&["f"]),
        &marker_lines(&[9]),
        &HashMap::new(),
        &reach_public(&["f"]),
    );
    assert_eq!(out.len(), 1, "out-of-window marker attaches to nothing");
    assert_eq!(out[0].line, 9, "anchored at the marker line");
    assert!(out[0]
        .reason
        .clone()
        .unwrap_or_default()
        .contains("not attached"));
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

// ── markers that attach to no function at all ────────────────────────────

#[test]
fn marker_not_attached_to_any_function_is_reported() {
    // `qual:api` on a `pub use` re-export, a struct, or a const: both markers
    // only affect function-level checks (DRY-002, TQ-003), so one that reaches
    // no function provably does nothing. Without this the marker would stay an
    // unverified silencer forever — exactly what this feature removes.
    let out = detect_stale_marker_orphans(
        &[], // no declared function claims it
        &names(&[]),
        &marker_lines(&[9]), // a lone `// qual:api` at line 9
        &HashMap::new(),
        &reach_public(&[]),
    );
    assert_eq!(
        out.len(),
        1,
        "an unattached marker must be reported: {out:?}"
    );
    assert_eq!(out[0].marker, MarkerKind::Api);
    assert_eq!(out[0].line, 9);
    let reason = out[0].reason.clone().unwrap_or_default();
    assert!(
        reason.contains("not attached") && reason.contains("remove"),
        "must say it reaches no function and to remove it: {reason}"
    );
}

#[test]
fn unattached_test_helper_marker_is_reported_too() {
    let out = detect_stale_marker_orphans(
        &[],
        &names(&[]),
        &HashMap::new(),
        &marker_lines(&[9]),
        &reach_public(&[]),
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].marker, MarkerKind::TestHelper);
}

#[test]
fn marker_attached_within_the_annotation_window_is_not_called_unattached() {
    // Attachment must mirror `mark_api_declarations` exactly (same window), or
    // we would tell the author to delete a marker that is actually working.
    // Marker at 9, fn at 12 → distance 3 = the window edge, still attached.
    let out = detect_stale_marker_orphans(
        &[declared("f", 12, true, false)],
        &names(&[]),
        &marker_lines(&[9]),
        &HashMap::new(),
        &reach_public(&["f"]),
    );
    assert!(
        out.is_empty(),
        "reachable + uncalled + attached ⇒ the marker is doing its job: {out:?}"
    );
}

#[test]
fn marker_on_an_exempt_function_is_reported_as_ineffective() {
    // A trait-impl method is excluded from DRY-002 and TQ-003 regardless, so
    // the marker changes nothing there either.
    let mut d = declared("fmt", 10, true, false);
    d.is_trait_impl = true;
    let out = detect_stale_marker_orphans(
        &[d],
        &names(&[]),
        &marker_lines(&[9]),
        &HashMap::new(),
        &reach_public(&["fmt"]),
    );
    assert_eq!(out.len(), 1, "marker on an exempt fn does nothing: {out:?}");
    assert!(
        out[0]
            .reason
            .clone()
            .unwrap_or_default()
            .contains("already exempt"),
        "must explain that the function is exempt anyway"
    );
}
