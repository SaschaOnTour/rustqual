//! The bare markers on a type or constant.
//!
//! Before DRY-006 a `qual:api` here could only ever be reported as "attached to
//! no function", because nothing acted on types at all. Now it means the same
//! as on a function — "consumers live outside the analysed code" — and is
//! verified the same way.

use super::*;

/// `MarkerContext` over one declared type, with the given production reference
/// set and reachability.
fn detect_type(
    declared: &[DeclaredType],
    prod_refs: &HashSet<String>,
    api_lines: &HashMap<String, HashSet<usize>>,
    reach: &ExternalReach,
) -> Vec<OrphanSuppression> {
    crate::app::stale_markers::detect_stale_marker_orphans(&MarkerContext {
        declared_fns: &[],
        declared_types: declared,
        prod_calls: &HashSet::new(),
        prod_refs,
        api_lines,
        test_helper_lines: &HashMap::new(),
        reach,
    })
}

fn public_entry() -> ExternalReach {
    reach_of(&[("src/lib.rs", "pub struct Entry;")])
}

#[test]
fn a_marker_on_a_type_is_no_longer_reported_as_unattached() {
    // The regression this whole feature closes: the marker used to be inert on
    // a type, so it was reported as reaching nothing.
    let out = detect_type(
        &[declared_type("Entry", 10, true, false)],
        &HashSet::new(),
        &marker_lines(&[9]),
        &public_entry(),
    );
    assert!(
        out.is_empty(),
        "a working qual:api on a public, unused type is silent: {out:?}"
    );
}

#[test]
fn a_marker_on_a_type_production_uses_is_spent() {
    let out = detect_type(
        &[declared_type("Entry", 10, true, false)],
        &names(&["Entry"]),
        &marker_lines(&[9]),
        &public_entry(),
    );
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].marker, MarkerKind::Api);
    assert_eq!(
        out[0].line, 9,
        "anchored at the marker, not the declaration"
    );
    let reason = out[0].reason.clone().unwrap_or_default();
    assert!(
        reason.contains("refers to") && reason.contains("Entry"),
        "the message must use the verb that fits a type: {reason}"
    );
}

#[test]
fn a_marker_on_a_crate_internal_type_never_applied() {
    // No outside consumer can name it, so the marker's premise is false —
    // the same category error as on a function behind a private module.
    let reach = reach_of(&[
        ("src/lib.rs", "mod inner;"),
        ("src/inner.rs", "pub struct Hidden;"),
    ]);
    let mut d = declared_type("Hidden", 10, true, false);
    d.file = "src/inner.rs".to_string();
    let mut lines = HashMap::new();
    lines.insert("src/inner.rs".to_string(), [9].into_iter().collect());
    let out = detect_type(&[d], &HashSet::new(), &lines, &reach);
    assert_eq!(out.len(), 1, "{out:?}");
    let reason = out[0].reason.clone().unwrap_or_default();
    assert!(
        reason.contains("outside the crate"),
        "must report the never-applied case: {reason}"
    );
}

#[test]
fn a_marker_on_an_exempt_type_changes_nothing() {
    // `#[allow(dead_code)]` already excludes it from DRY-006, so the marker
    // cannot be doing any work.
    let mut d = declared_type("Kept", 10, true, false);
    d.has_allow_dead_code = true;
    let out = detect_type(&[d], &HashSet::new(), &marker_lines(&[9]), &public_entry());
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0]
        .reason
        .clone()
        .unwrap_or_default()
        .contains("already exempt"));
}

#[test]
fn a_name_shared_with_a_function_is_not_attributable() {
    // Both evidence sets are keyed by bare name, so a call to a function
    // `Entry` would otherwise vouch for a type `Entry` and mark a working
    // marker spent.
    let out = crate::app::stale_markers::detect_stale_marker_orphans(&MarkerContext {
        declared_fns: &[declared("Entry", 20, false, false)],
        declared_types: &[declared_type("Entry", 10, true, false)],
        prod_calls: &HashSet::new(),
        prod_refs: &names(&["Entry"]),
        api_lines: &marker_lines(&[9]),
        test_helper_lines: &HashMap::new(),
        reach: &public_entry(),
    });
    assert!(
        out.is_empty(),
        "an unattributable use must not mark the marker spent: {out:?}"
    );
}

#[test]
fn a_test_helper_marker_on_a_type_is_verified_too() {
    // Judged by use only — being crate-internal is a helper's normal state.
    let out = crate::app::stale_markers::detect_stale_marker_orphans(&MarkerContext {
        declared_fns: &[],
        declared_types: &[declared_type("Fixture", 10, false, true)],
        prod_calls: &HashSet::new(),
        prod_refs: &names(&["Fixture"]),
        api_lines: &HashMap::new(),
        test_helper_lines: &marker_lines(&[9]),
        reach: &reach_of(&[("src/lib.rs", "mod hidden;")]),
    });
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].marker, MarkerKind::TestHelper);
}

#[test]
fn a_marker_on_neither_a_function_nor_a_type_is_still_unattached() {
    // A `pub use` re-export, a module, a trait — the check must keep naming
    // those, or the marker becomes an unverified silencer again.
    let out = detect_type(&[], &HashSet::new(), &marker_lines(&[9]), &public_entry());
    assert_eq!(out.len(), 1, "{out:?}");
    let reason = out[0].reason.clone().unwrap_or_default();
    assert!(
        reason.contains("not attached") && reason.contains("re-export"),
        "must name what it could be sitting on: {reason}"
    );
}

#[test]
fn a_marker_belongs_to_the_nearest_declaration() {
    // The annotation window is a flat look-back, so a marker on a short type
    // can also fall within reach of the next declaration below it. Judging both
    // lets the further one's verdict be reported against a marker that is not
    // its own: here `helper` is called by production, and reporting the marker
    // as spent would tell the author to delete a working `qual:api` on `Exposed`.
    let out = crate::app::stale_markers::detect_stale_marker_orphans(&MarkerContext {
        declared_fns: &[declared("helper", 4, true, false)],
        declared_types: &[declared_type("Exposed", 2, true, false)],
        prod_calls: &names(&["helper"]),
        prod_refs: &HashSet::new(),
        api_lines: &marker_lines(&[1]),
        test_helper_lines: &HashMap::new(),
        reach: &reach_of(&[(
            "src/lib.rs",
            "pub struct Exposed; pub fn helper() -> u8 { 1 }",
        )]),
    });
    assert!(
        out.is_empty(),
        "the marker belongs to `Exposed` two lines below it, not to `helper`: {out:?}"
    );
}
