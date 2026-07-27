//! Per-dimension GitHub annotation *message* tests — the detail-match arms in
//! `format_{dry,srp,coupling}_message`, `complexity_level`, and the `located`
//! no-location branch that the smoke tests don't reach.
use super::*;
use crate::domain::findings::{
    ComplexityFinding, ComplexityFindingKind, CouplingFinding, CouplingFindingDetails,
    CouplingFindingKind, DryFinding, DryFindingDetails, DryFindingKind, DuplicateParticipant,
    FragmentParticipant, SrpFinding, SrpFindingDetails, SrpFindingKind, TqFinding, TqFindingKind,
};
use crate::domain::{Dimension, Severity};

fn common(dim: Dimension, file: &str, suppressed: bool) -> Finding {
    Finding {
        file: file.into(),
        line: 5,
        column: 0,
        dimension: dim,
        rule_id: "r".into(),
        message: "fallback".into(),
        severity: Severity::Medium,
        suppressed,
    }
}

fn dry(kind: DryFindingKind, details: DryFindingDetails) -> DryFinding {
    DryFinding {
        common: common(Dimension::Dry, "d.rs", false),
        kind,
        details,
    }
}

#[test]
fn dry_messages_per_kind_and_suppressed_dropped() {
    let dup = dry(
        DryFindingKind::DuplicateExact,
        DryFindingDetails::Duplicate {
            participants: vec![DuplicateParticipant {
                function_name: "dupfn".into(),
                file: "d.rs".into(),
                line: 1,
            }],
            similarity: None,
        },
    );
    let frag = dry(
        DryFindingKind::Fragment,
        DryFindingDetails::Fragment {
            participants: vec![FragmentParticipant {
                function_name: "fragfn".into(),
                file: "f.rs".into(),
                line: 2,
                end_line: 6,
            }],
            statement_count: 4,
        },
    );
    let mut suppressed = dup.clone();
    suppressed.common.suppressed = true;
    let out = render_dry_chunk(&[dup, frag, suppressed]);
    assert!(out.contains("Duplicate functions: dupfn"), "{out}");
    assert!(
        out.contains("Duplicate fragment (4 stmts): fragfn"),
        "{out}"
    );
    // Only two rows rendered (suppressed dropped): two annotation lines.
    assert_eq!(out.lines().count(), 2, "suppressed dropped: {out}");
}

fn srp(kind: SrpFindingKind, details: SrpFindingDetails) -> SrpFinding {
    SrpFinding {
        common: common(Dimension::Srp, "s.rs", false),
        kind,
        details,
    }
}

#[test]
fn srp_messages_per_kind() {
    let out = render_srp_chunk(&[
        srp(
            SrpFindingKind::StructCohesion,
            SrpFindingDetails::StructCohesion {
                struct_name: "Big".into(),
                lcom4: 4,
                field_count: 9,
                method_count: 7,
                fan_out: 2,
                composite_score: 0.0,
                clusters: vec![],
            },
        ),
        srp(
            SrpFindingKind::ModuleLength,
            SrpFindingDetails::ModuleLength {
                module: "big_mod".into(),
                production_lines: 900,
                independent_clusters: 3,
                cluster_names: vec![],
                length_score: 0.0,
            },
        ),
        srp(
            SrpFindingKind::ParameterCount,
            SrpFindingDetails::ParameterCount {
                function_name: "f".into(),
                parameter_count: 7,
            },
        ),
    ]);
    assert!(
        out.contains("SRP cohesion: Big has LCOM4=4, methods=7"),
        "{out}"
    );
    assert!(
        out.contains("SRP module length: big_mod has 900 lines"),
        "module length message: {out}"
    );
    assert!(out.contains("'f' has 7 parameters"), "{out}");
}

fn cpl(details: CouplingFindingDetails, file: &str) -> CouplingFinding {
    CouplingFinding {
        common: common(Dimension::Coupling, file, false),
        kind: CouplingFindingKind::Structural,
        details,
    }
}

#[test]
fn coupling_messages_and_cycle_has_no_file_location() {
    // A cycle finding carries an empty file → `located` emits the no-location
    // form `::level::msg` (pins `file.is_empty() || line == 0`).
    let out = render_coupling_chunk(&[
        cpl(
            CouplingFindingDetails::Cycle {
                modules: vec!["a".into(), "b".into()],
            },
            "",
        ),
        cpl(
            CouplingFindingDetails::SdpViolation {
                from_module: "x".into(),
                to_module: "y".into(),
                from_instability: 0.2,
                to_instability: 0.8,
            },
            "c.rs",
        ),
        cpl(
            CouplingFindingDetails::ThresholdExceeded {
                module_name: "hot_mod".into(),
                afferent: 1,
                efferent: 9,
                instability: 0.9,
            },
            "t.rs",
        ),
    ]);
    assert!(out.contains("Coupling cycle: a → b"), "{out}");
    assert!(
        out.contains("::warning::Coupling cycle"),
        "no-location form: {out}"
    );
    assert!(out.contains("SDP violation: x"), "{out}");
    assert!(out.contains("file=c.rs"), "located form for SDP: {out}");
    assert!(
        out.contains("Coupling threshold exceeded: hot_mod"),
        "threshold message: {out}"
    );
}

#[test]
fn complexity_level_maps_kind_to_annotation_level() {
    // Threshold breaches → ::notice, smells → ::warning (pins complexity_level).
    let cx = |kind| ComplexityFinding {
        common: common(Dimension::Complexity, "c.rs", false),
        kind,
        metric_value: 10,
        threshold: 5,
        hotspot: None,
    };
    let cog = cx(ComplexityFindingKind::Cognitive);
    let magic = cx(ComplexityFindingKind::MagicNumber);
    assert!(
        render_complexity_chunk(&[cog]).contains("::notice"),
        "cognitive → notice"
    );
    assert!(
        render_complexity_chunk(&[magic]).contains("::warning"),
        "magic → warning"
    );
}

#[test]
fn tq_view_drops_suppressed_rows() {
    let live = TqFinding {
        common: common(Dimension::TestQuality, "live.rs", false),
        kind: TqFindingKind::NoAssertion,
        function_name: "t".into(),
        uncovered_lines: None,
        coverage: crate::domain::findings::CoverageEvidence::NotApplicable,
    };
    let mut suppressed = live.clone();
    suppressed.common.file = "supp.rs".into();
    suppressed.common.suppressed = true;
    let out = render_tq_chunk(&[live, suppressed]);
    // The live row is emitted, the suppressed one dropped — distinct files so a
    // deleted `!suppressed` (which would keep the suppressed one) is caught.
    assert!(out.contains("file=live.rs"), "live row emitted: {out}");
    assert!(
        !out.contains("file=supp.rs"),
        "suppressed row dropped: {out}"
    );
}
