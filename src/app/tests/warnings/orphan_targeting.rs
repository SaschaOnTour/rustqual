//! Target-awareness of the orphan detector: a targeted `allow(dim, t)` marker
//! is verified against a finding of *that exact kind* (not just any finding of
//! the dimension), per-component for SRP module findings, and module-global
//! coupling pins are treated as unverifiable rather than false-flagged.

use super::orphan_target_helpers::*;
use super::*;
use crate::domain::findings::OrphanKind;
use crate::findings::Dimension;

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

// ── component-gating: a pin on the inactive component is stale ──

#[test]
fn cluster_pin_on_length_only_finding_is_orphan() {
    // Length component active (score 1.5), cohesion inactive (1 cluster <= max 2).
    // A max_independent_clusters pin silences nothing → stale orphan.
    let out = orphans(
        Dimension::Srp,
        "max_independent_clusters",
        Some(5.0),
        1,
        |a| {
            a.findings
                .srp
                .push(srp_module_full("src/x.rs", 350, 1.5, 1));
        },
    );
    assert_eq!(
        out.len(),
        1,
        "cluster pin on a length-only finding must be flagged, got {out:?}"
    );
    assert_eq!(
        out[0].kind,
        OrphanKind::Stale,
        "an inactive-component pin is stale: {out:?}"
    );
}

#[test]
fn file_length_pin_on_cluster_only_finding_is_orphan() {
    // Cohesion component active (5 clusters > max 2), length inactive (score 0.5).
    // A file_length pin silences nothing → stale orphan.
    let out = orphans(Dimension::Srp, "file_length", Some(400.0), 1, |a| {
        a.findings
            .srp
            .push(srp_module_full("src/x.rs", 200, 0.5, 5));
    });
    assert_eq!(
        out.len(),
        1,
        "file_length pin on a cluster-only finding must be flagged, got {out:?}"
    );
    assert_eq!(
        out[0].kind,
        OrphanKind::Stale,
        "an inactive-component pin is stale: {out:?}"
    );
}

// ── magic_numbers position anchors at the function line ────────

#[test]
fn magic_numbers_pin_above_function_is_not_orphan() {
    // A `allow(complexity, magic_numbers)` marker sits above the function
    // (line 10); the magic literal is deep in the body (line 25). The
    // suppression attaches to the function, so the orphan position must also
    // anchor at the function line — otherwise the valid marker is falsely
    // reported stale because the literal is outside the marker's window.
    let m = ComplexityMetrics {
        magic_numbers: vec![crate::adapters::analyzers::iosp::MagicNumberOccurrence {
            line: 25,
            value: "42".to_string(),
        }],
        ..Default::default()
    };
    let out = orphans(Dimension::Complexity, "magic_numbers", None, 10, |a| {
        a.results = vec![make_fa_with_complexity("src/x.rs", 10, m.clone())];
    });
    assert!(
        out.is_empty(),
        "magic_numbers marker on the fn must match, got {out:?}"
    );
}

// ── targeted coupling markers are module-global → unverifiable ─

#[test]
fn targeted_coupling_pin_with_structural_finding_is_not_orphan() {
    // A `allow(coupling, max_fan_out=N)` pins a module-global metric with no
    // line-anchored position. The file also trips a structural coupling
    // finding (OI/SIT/DEH/IET, target=None). The structural finding must NOT
    // make the targeted pin "verifiable" and then unmatched → it must be
    // skipped, not falsely reported as a stale orphan.
    let out = orphans(Dimension::Coupling, "max_fan_out", Some(10.0), 5, |a| {
        a.findings
            .coupling
            .push(make_structural_coupling_finding("src/x.rs", 5));
    });
    assert!(
        out.is_empty(),
        "targeted coupling pin is module-global, not orphan: {out:?}"
    );
}

#[test]
fn targeted_coupling_sdp_pin_without_finding_is_not_orphan() {
    // A boolean coupling target (sdp) is likewise module-global; with no
    // coupling position at all the marker is simply unverifiable, not orphan.
    let out = orphans(Dimension::Coupling, "sdp", None, 5, |_a| {});
    assert!(
        out.is_empty(),
        "module-global coupling marker is unverifiable: {out:?}"
    );
}

#[test]
fn coupling_metric_pin_is_never_too_loose_checked() {
    // Coupling metrics are module-global (no line-anchored position), so a
    // too-loose `allow(coupling, max_instability=0.99)` over a real I=0.83 can
    // never be flagged — a deliberate, documented blind spot. Pins the contract
    // so a future change to `is_verifiable` is noticed.
    let out = orphans(Dimension::Coupling, "max_instability", Some(0.99), 5, |a| {
        a.findings
            .coupling
            .push(make_structural_coupling_finding("src/x.rs", 5));
    });
    assert!(
        out.is_empty(),
        "coupling metric pins are not too-loose-checked: {out:?}"
    );
}

#[test]
fn structural_coupling_target_matches_its_finding() {
    // A structural coupling target (oi/sit/deh/iet) IS line-anchored, so
    // `allow(coupling, sit)` next to a SIT finding matches → not orphan.
    let out = orphans(Dimension::Coupling, "sit", None, 5, |a| {
        a.findings
            .coupling
            .push(make_structural_coupling_finding("src/x.rs", 5));
    });
    assert!(
        out.is_empty(),
        "allow(coupling, sit) matches a SIT finding: {out:?}"
    );
}

#[test]
fn structural_coupling_target_wrong_kind_is_orphan() {
    // The finding is SIT; an `oi` marker matches no position → stale orphan
    // (and a structural target is verifiable, unlike a module-global metric).
    let out = orphans(Dimension::Coupling, "oi", None, 5, |a| {
        a.findings
            .coupling
            .push(make_structural_coupling_finding("src/x.rs", 5));
    });
    assert_eq!(
        out.len(),
        1,
        "allow(coupling, oi) must not match a SIT finding: {out:?}"
    );
}

// ── both module-length components active ───────────────────────

#[test]
fn both_active_module_finding_emits_both_targets() {
    // length AND cohesion both active (900 lines / score 1.5, 5 clusters > max
    // 2): a tight file_length pin and a tight max_independent_clusters pin are
    // each non-stale; a god_struct pin (wrong kind) is still an orphan.
    let push = |a: &mut crate::report::AnalysisResult| {
        a.findings
            .srp
            .push(srp_module_full("src/x.rs", 900, 1.5, 5));
    };
    assert!(orphans(Dimension::Srp, "file_length", Some(900.0), 1, push).is_empty());
    assert!(orphans(
        Dimension::Srp,
        "max_independent_clusters",
        Some(5.0),
        1,
        push
    )
    .is_empty());
    assert_eq!(
        orphans(Dimension::Srp, "god_struct", None, 1, push).len(),
        1
    );
}

// ── target-awareness for arch / dry / tq targeted markers ──────

fn arch_finding(
    file: &str,
    line: usize,
    rule_id: &str,
) -> crate::domain::findings::ArchitectureFinding {
    crate::domain::findings::ArchitectureFinding {
        common: make_finding(file, line, Dimension::Architecture, rule_id),
    }
}

fn arch_enabled() -> Config {
    let mut c = Config::default();
    c.architecture.enabled = true;
    c
}

#[test]
fn architecture_targeted_wrong_family_is_orphan() {
    // Only finding is a `forbidden` edge; a `layer` pin matches no position.
    let cfg = arch_enabled();
    let out = orphans_cfg(
        target_of("layer", None),
        Dimension::Architecture,
        5,
        &cfg,
        |a| {
            a.findings
                .architecture
                .push(arch_finding("src/x.rs", 5, "architecture/forbidden"));
        },
    );
    assert_eq!(
        out.len(),
        1,
        "layer marker must not match a forbidden finding: {out:?}"
    );
}

#[test]
fn architecture_targeted_right_family_is_clean() {
    let cfg = arch_enabled();
    let out = orphans_cfg(
        target_of("forbidden", None),
        Dimension::Architecture,
        5,
        &cfg,
        |a| {
            a.findings.architecture.push(arch_finding(
                "src/x.rs",
                5,
                "architecture/forbidden/edge",
            ));
        },
    );
    assert!(
        out.is_empty(),
        "forbidden marker matches a forbidden finding: {out:?}"
    );
}

#[test]
fn tq_targeted_wrong_kind_is_orphan() {
    use crate::domain::findings::TqFindingKind;
    let out = orphans(Dimension::TestQuality, "uncovered", None, 5, |a| {
        a.findings
            .test_quality
            .push(make_tq_finding("src/x.rs", 5, TqFindingKind::NoAssertion));
    });
    assert_eq!(
        out.len(),
        1,
        "uncovered marker must not match a no_assertion finding: {out:?}"
    );
}

#[test]
fn tq_targeted_right_kind_is_clean() {
    use crate::domain::findings::TqFindingKind;
    let out = orphans(Dimension::TestQuality, "no_assertion", None, 5, |a| {
        a.findings
            .test_quality
            .push(make_tq_finding("src/x.rs", 5, TqFindingKind::NoAssertion));
    });
    assert!(
        out.is_empty(),
        "no_assertion marker matches its own kind: {out:?}"
    );
}

#[test]
fn dry_targeted_wrong_kind_is_orphan() {
    // Only finding is a wildcard import; a `duplicate` pin matches nothing.
    let out = orphans(Dimension::Dry, "duplicate", None, 5, |a| {
        a.findings
            .dry
            .push(make_dry_wildcard_finding("src/x.rs", 5));
    });
    assert_eq!(
        out.len(),
        1,
        "duplicate marker must not match a wildcard finding: {out:?}"
    );
}
