//! Too-loose-pin orphan detection: a metric pin parked further above the
//! value it covers than `[suppression].pin_headroom` (default 10%) is
//! reported; a pin within headroom, or too tight (re-firing), is healthy.

use super::orphan_target_helpers::*;
use super::*;
use crate::domain::findings::OrphanKind;
use crate::findings::Dimension;

#[test]
fn file_length_pin_within_headroom_is_clean() {
    // 900 lines, pin 950 → 950 <= 900*1.10 (990) → accepted.
    let out = orphans(Dimension::Srp, "file_length", Some(950.0), 1, |a| {
        a.findings.srp.push(srp_module("src/x.rs", 900));
    });
    assert!(
        out.is_empty(),
        "pin within 10% headroom is fine, got {out:?}"
    );
}

#[test]
fn file_length_pin_too_loose_is_orphan() {
    // 900 lines, pin 1100 → 1100 > 990 → too loose.
    let out = orphans(Dimension::Srp, "file_length", Some(1100.0), 1, |a| {
        a.findings.srp.push(srp_module("src/x.rs", 900));
    });
    assert_eq!(out.len(), 1, "pin 1100 over value 900 must be too-loose");
    let reason = out[0].reason.as_deref().unwrap_or("");
    assert!(
        reason.contains("900"),
        "reason should name the value: {reason}"
    );
    assert!(
        reason.contains("tighten"),
        "reason should advise tightening: {reason}"
    );
}

#[test]
fn complexity_cognitive_pin_too_loose_is_orphan() {
    let m = ComplexityMetrics {
        cognitive_complexity: 18,
        ..Default::default()
    };
    let out = orphans(
        Dimension::Complexity,
        "max_cognitive",
        Some(30.0),
        10,
        |a| {
            a.results = vec![make_fa_with_complexity("src/x.rs", 10, m.clone())];
        },
    );
    assert_eq!(out.len(), 1, "pin 30 over cognitive 18 must be too-loose");
}

#[test]
fn complexity_cognitive_pin_within_headroom_is_clean() {
    // cognitive 18, pin 19 → 19 <= 18*1.10 (19.8) → accepted.
    let m = ComplexityMetrics {
        cognitive_complexity: 18,
        ..Default::default()
    };
    let out = orphans(
        Dimension::Complexity,
        "max_cognitive",
        Some(19.0),
        10,
        |a| {
            a.results = vec![make_fa_with_complexity("src/x.rs", 10, m.clone())];
        },
    );
    assert!(out.is_empty(), "pin within headroom is fine, got {out:?}");
}

#[test]
fn complexity_pin_too_tight_refires_not_orphan() {
    // cognitive 18, pin 16 → finding re-fires above the pin; the pin is
    // legitimately limiting, not stale → no orphan.
    let m = ComplexityMetrics {
        cognitive_complexity: 18,
        ..Default::default()
    };
    let out = orphans(
        Dimension::Complexity,
        "max_cognitive",
        Some(16.0),
        10,
        |a| {
            a.results = vec![make_fa_with_complexity("src/x.rs", 10, m.clone())];
        },
    );
    assert!(
        out.is_empty(),
        "too-tight (re-firing) pin is not an orphan, got {out:?}"
    );
}

// ── too-loose for the other SRP metric targets ─────────────────

#[test]
fn cluster_pin_too_loose_is_orphan() {
    // cohesion active (5 clusters > max 2); pin 10 > 5*1.10 → too loose.
    let out = orphans(
        Dimension::Srp,
        "max_independent_clusters",
        Some(10.0),
        1,
        |a| {
            a.findings
                .srp
                .push(srp_module_full("src/x.rs", 200, 0.5, 5));
        },
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].kind, OrphanKind::PinTooLoose, "{out:?}");
    assert!(out[0].reason.as_deref().unwrap_or("").contains('5'));
}

#[test]
fn cluster_pin_within_headroom_is_clean() {
    // 5 clusters, pin 5 → covered and 5 <= 5*1.10 → fine.
    let out = orphans(
        Dimension::Srp,
        "max_independent_clusters",
        Some(5.0),
        1,
        |a| {
            a.findings
                .srp
                .push(srp_module_full("src/x.rs", 200, 0.5, 5));
        },
    );
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn max_parameters_pin_too_loose_is_orphan() {
    // 7-param function (make_srp_param_finding); pin 20 > 7*1.10 → too loose.
    let out = orphans(Dimension::Srp, "max_parameters", Some(20.0), 5, |a| {
        a.findings
            .srp
            .push(make_srp_param_finding("src/x.rs", 5, false));
    });
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].kind, OrphanKind::PinTooLoose, "{out:?}");
}

#[test]
fn max_parameters_pin_within_headroom_is_clean() {
    let out = orphans(Dimension::Srp, "max_parameters", Some(7.0), 5, |a| {
        a.findings
            .srp
            .push(make_srp_param_finding("src/x.rs", 5, false));
    });
    assert!(out.is_empty(), "{out:?}");
}

// ── headroom boundary + config-driven headroom ─────────────────

#[test]
fn pin_exactly_at_headroom_ceiling_is_clean() {
    // cognitive 18, pin 19.8 == 18*1.10 exactly → strict `>` keeps it clean.
    let m = ComplexityMetrics {
        cognitive_complexity: 18,
        ..Default::default()
    };
    let out = orphans(
        Dimension::Complexity,
        "max_cognitive",
        Some(19.8),
        10,
        |a| {
            a.results = vec![make_fa_with_complexity("src/x.rs", 10, m.clone())];
        },
    );
    assert!(
        out.is_empty(),
        "pin exactly at the ceiling is healthy: {out:?}"
    );
}

#[test]
fn pin_just_above_headroom_ceiling_is_orphan() {
    let m = ComplexityMetrics {
        cognitive_complexity: 18,
        ..Default::default()
    };
    let out = orphans(
        Dimension::Complexity,
        "max_cognitive",
        Some(19.81),
        10,
        |a| {
            a.results = vec![make_fa_with_complexity("src/x.rs", 10, m.clone())];
        },
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].kind, OrphanKind::PinTooLoose, "{out:?}");
}

#[test]
fn pin_headroom_config_flips_the_verdict() {
    // cognitive 18, pin 20. Default headroom 0.10 (ceiling 19.8) → too loose;
    // a 0.20 headroom (ceiling 21.6) accepts the very same pin.
    let m = ComplexityMetrics {
        cognitive_complexity: 18,
        ..Default::default()
    };
    let seed = |a: &mut crate::report::AnalysisResult| {
        a.results = vec![make_fa_with_complexity("src/x.rs", 10, m.clone())];
    };
    let tight = orphans(Dimension::Complexity, "max_cognitive", Some(20.0), 10, seed);
    assert_eq!(tight.len(), 1, "default 10% headroom flags pin 20 over 18");

    let mut cfg = Config::default();
    cfg.suppression.pin_headroom = 0.20;
    let loose = orphans_cfg(
        target_of("max_cognitive", Some(20.0)),
        Dimension::Complexity,
        10,
        &cfg,
        seed,
    );
    assert!(
        loose.is_empty(),
        "20% headroom accepts pin 20 over 18: {loose:?}"
    );
}

#[test]
fn too_loose_uses_largest_covered_value_not_smallest() {
    // Two cognitive findings under one marker (16 and 18); pin 19 is tight to
    // the larger (18) — must stay clean. A min-fold regression would compare
    // against 16 and wrongly flag it.
    let lo = ComplexityMetrics {
        cognitive_complexity: 16,
        ..Default::default()
    };
    let hi = ComplexityMetrics {
        cognitive_complexity: 18,
        ..Default::default()
    };
    let out = orphans(
        Dimension::Complexity,
        "max_cognitive",
        Some(19.0),
        10,
        |a| {
            a.results = vec![
                make_fa_with_complexity("src/x.rs", 10, lo.clone()),
                make_fa_with_complexity("src/x.rs", 11, hi.clone()),
            ];
        },
    );
    assert!(
        out.is_empty(),
        "pin tight to the largest covered value is clean: {out:?}"
    );
}

#[test]
fn orphan_target_rendering_covers_metric_boolean_and_blanket() {
    use crate::domain::findings::{OrphanKind, OrphanSuppression};
    use crate::domain::SuppressionTarget;
    let mk = |target| OrphanSuppression {
        file: "x".into(),
        line: 1,
        dimensions: vec![Dimension::Srp],
        target,
        reason: None,
        kind: OrphanKind::Stale,
    };
    let metric = mk(Some(SuppressionTarget::Metric {
        name: "file_length".into(),
        pin: 400.0,
    }));
    assert_eq!(metric.target_spec().as_deref(), Some("file_length=400"));
    assert_eq!(metric.target_suffix(), ", file_length=400");
    // A large finite pin must not saturate (no `as i64` cast).
    let big = mk(Some(SuppressionTarget::Metric {
        name: "file_length".into(),
        pin: 1e15,
    }));
    assert_eq!(
        big.target_spec().as_deref(),
        Some("file_length=1000000000000000")
    );
    let boolean = mk(Some(SuppressionTarget::Boolean {
        name: "god_struct".into(),
    }));
    assert_eq!(boolean.target_spec().as_deref(), Some("god_struct"));
    assert_eq!(boolean.target_suffix(), ", god_struct");
    let blanket = mk(None);
    assert_eq!(blanket.target_spec(), None);
    assert_eq!(blanket.target_suffix(), "");
}
