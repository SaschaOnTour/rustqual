//! Orphan detection for *targeted* suppressions: a targeted `allow(dim, t)`
//! marker is verified against a finding of that exact target kind (not just
//! any finding of the dimension), and a metric pin parked far above the
//! value it covers is reported as a too-loose orphan (`[suppression].
//! pin_headroom`, default 10%).
use super::*;
use crate::domain::findings::OrphanSuppression;
use crate::domain::SuppressionTarget;
use crate::findings::{Dimension, Suppression};

/// Run the orphan detector for a single targeted marker on `src/x.rs`.
fn orphans(
    dim: Dimension,
    target: &str,
    pin: Option<f64>,
    sup_line: usize,
    mut seed: impl FnMut(&mut crate::report::AnalysisResult),
) -> Vec<OrphanSuppression> {
    let mut sups = HashMap::new();
    sups.insert(
        "src/x.rs".to_string(),
        vec![Suppression {
            line: sup_line,
            dimensions: vec![dim],
            reason: Some("r".to_string()),
            target: Some(SuppressionTarget {
                name: target.to_string(),
                pin,
            }),
        }],
    );
    let mut analysis = empty_analysis();
    seed(&mut analysis);
    crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &std::collections::HashMap::new(),
        &analysis,
        &Config::default(),
    )
}

fn srp_module(file: &str, production_lines: usize) -> crate::domain::findings::SrpFinding {
    let mut f = make_srp_module_finding(file);
    if let crate::domain::findings::SrpFindingDetails::ModuleLength {
        production_lines: pl,
        ..
    } = &mut f.details
    {
        *pl = production_lines;
    }
    f
}

// ── target-awareness ───────────────────────────────────────────

#[test]
fn file_length_pin_mismatches_god_struct_is_orphan() {
    // The only SRP finding is a god-struct (StructCohesion); a file_length
    // pin targets a different kind, so it matches nothing → orphan.
    let out = orphans(Dimension::Srp, "file_length", Some(400.0), 5, |a| {
        a.findings.srp.push(make_srp_struct_finding("src/x.rs", 5));
    });
    assert_eq!(out.len(), 1, "file_length pin must not match god_struct");
}

#[test]
fn complexity_wrong_metric_target_is_orphan() {
    // Function trips cognitive only; a max_cyclomatic pin matches nothing.
    let m = ComplexityMetrics {
        cognitive_complexity: 18,
        ..Default::default()
    };
    let out = orphans(
        Dimension::Complexity,
        "max_cyclomatic",
        Some(20.0),
        10,
        |a| {
            a.results = vec![make_fa_with_complexity("src/x.rs", 10, m.clone())];
        },
    );
    assert_eq!(
        out.len(),
        1,
        "max_cyclomatic pin must not match a cognitive finding"
    );
}

// ── too-loose pin ──────────────────────────────────────────────

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
