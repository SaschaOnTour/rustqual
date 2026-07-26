//! `OrphanSuppression::marker_spec` — the one place that names a reported
//! marker, so every reporter prints the same thing.

use crate::domain::findings::{MarkerKind, OrphanKind, OrphanSuppression};
use crate::domain::{Dimension, SuppressionTarget};

fn orphan(marker: MarkerKind, target: Option<SuppressionTarget>) -> OrphanSuppression {
    OrphanSuppression {
        marker,
        file: "src/lib.rs".into(),
        line: 1,
        dimensions: vec![Dimension::Dry],
        target,
        reason: None,
        kind: OrphanKind::Stale,
    }
}

#[test]
fn allow_marker_renders_scope_and_target() {
    let o = orphan(
        MarkerKind::Allow,
        Some(SuppressionTarget::Boolean {
            name: "duplicate".into(),
        }),
    );
    assert_eq!(o.marker_spec("dry"), "qual:allow(dry, duplicate)");
}

#[test]
fn allow_marker_without_target_renders_scope_only() {
    assert_eq!(
        orphan(MarkerKind::Allow, None).marker_spec("dry"),
        "qual:allow(dry)"
    );
}

#[test]
fn allow_marker_renders_a_metric_pin() {
    let o = orphan(
        MarkerKind::Allow,
        Some(SuppressionTarget::Metric {
            name: "file_length".into(),
            pin: 400.0,
        }),
    );
    assert_eq!(o.marker_spec("srp"), "qual:allow(srp, file_length=400)");
}

#[test]
fn bare_markers_ignore_the_scope_entirely() {
    // A stale `qual:api` carries no dimensions and no target — printing
    // `qual:allow(<all>)` would send the author hunting for a suppression
    // that was never written.
    for (marker, expected) in [
        (MarkerKind::Api, "qual:api"),
        (MarkerKind::TestHelper, "qual:test_helper"),
    ] {
        let spec = orphan(marker, None).marker_spec("<all>");
        assert_eq!(spec, expected, "{marker:?} must render bare");
    }
}

#[test]
fn json_kind_is_stable_per_marker() {
    assert_eq!(MarkerKind::Allow.json_kind(), "allow");
    assert_eq!(MarkerKind::Api.json_kind(), "api");
    assert_eq!(MarkerKind::TestHelper.json_kind(), "test_helper");
}

#[test]
fn default_marker_is_allow() {
    // Keeps `qual:allow` the implicit case for every existing construction
    // site and for `..Default::default()` style fixtures.
    assert_eq!(MarkerKind::default(), MarkerKind::Allow);
}
