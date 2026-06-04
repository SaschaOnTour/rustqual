//! JSON-envelope section tests: every per-category `build_*` and its
//! `JsonChunk` field must reach the serialized output, and the `compose`
//! coupling/srp `None`-when-empty guards must keep a populated section.
use super::*;
use crate::domain::analysis_data::ModuleCouplingRecord;
use crate::domain::findings::{
    ArchitectureFinding, CouplingFinding, CouplingFindingDetails, CouplingFindingKind, DryFinding,
    DryFindingDetails, DryFindingKind, DuplicateParticipant, FragmentParticipant, SrpFinding,
    SrpFindingDetails, SrpFindingKind, TqFinding, TqFindingKind,
};
use crate::domain::{AnalysisData, AnalysisFindings, Dimension, Finding, Severity};
use crate::report::{AnalysisResult, Summary};

fn common(dim: Dimension) -> Finding {
    Finding {
        file: "lib.rs".into(),
        line: 3,
        column: 0,
        dimension: dim,
        rule_id: "r".into(),
        message: "m".into(),
        severity: Severity::Medium,
        suppressed: false,
    }
}

fn dry(kind: DryFindingKind, details: DryFindingDetails) -> DryFinding {
    DryFinding {
        common: common(Dimension::Dry),
        kind,
        details,
    }
}

fn srp(kind: SrpFindingKind, details: SrpFindingDetails) -> SrpFinding {
    SrpFinding {
        common: common(Dimension::Srp),
        kind,
        details,
    }
}

fn cpl(details: CouplingFindingDetails) -> CouplingFinding {
    CouplingFinding {
        common: common(Dimension::Coupling),
        kind: CouplingFindingKind::Structural,
        details,
    }
}

fn analysis_with(findings: AnalysisFindings, data: AnalysisData) -> AnalysisResult {
    AnalysisResult {
        results: vec![],
        summary: Summary::default(),
        findings,
        data,
    }
}

fn all_dry() -> Vec<DryFinding> {
    vec![
        dry(
            DryFindingKind::DuplicateExact,
            DryFindingDetails::Duplicate {
                participants: vec![DuplicateParticipant {
                    function_name: "d".into(),
                    file: "lib.rs".into(),
                    line: 1,
                }],
                similarity: None,
            },
        ),
        dry(
            DryFindingKind::DeadCodeUncalled,
            DryFindingDetails::DeadCode {
                qualified_name: "dead".into(),
                suggestion: None,
            },
        ),
        dry(
            DryFindingKind::Wildcard,
            DryFindingDetails::Wildcard {
                module_path: "m::*".into(),
            },
        ),
        dry(
            DryFindingKind::Boilerplate,
            DryFindingDetails::Boilerplate {
                pattern_id: "BP-001".into(),
                struct_name: None,
                suggestion: "s".into(),
            },
        ),
        dry(
            DryFindingKind::Fragment,
            DryFindingDetails::Fragment {
                participants: vec![FragmentParticipant {
                    function_name: "frag".into(),
                    file: "lib.rs".into(),
                    line: 2,
                    end_line: 6,
                }],
                statement_count: 4,
            },
        ),
    ]
}

fn structural_srp() -> SrpFinding {
    srp(
        SrpFindingKind::Structural,
        SrpFindingDetails::Structural {
            item_name: "Foo".into(),
            code: "SLM".into(),
            detail: "d".into(),
        },
    )
}

fn rich_findings() -> AnalysisFindings {
    AnalysisFindings {
        dry: all_dry(),
        srp: vec![
            srp(
                SrpFindingKind::ParameterCount,
                SrpFindingDetails::ParameterCount {
                    function_name: "f".into(),
                    parameter_count: 7,
                },
            ),
            structural_srp(),
        ],
        coupling: vec![
            cpl(CouplingFindingDetails::Cycle {
                modules: vec!["a".into(), "b".into()],
            }),
            cpl(CouplingFindingDetails::SdpViolation {
                from_module: "x".into(),
                to_module: "y".into(),
                from_instability: 0.2,
                to_instability: 0.8,
            }),
            cpl(CouplingFindingDetails::Structural {
                item_name: "Baz".into(),
                code: "DEH".into(),
                detail: "downcast".into(),
            }),
        ],
        test_quality: vec![TqFinding {
            common: common(Dimension::TestQuality),
            kind: TqFindingKind::NoAssertion,
            function_name: "t".into(),
            uncovered_lines: None,
        }],
        architecture: vec![ArchitectureFinding {
            common: common(Dimension::Architecture),
        }],
        ..Default::default()
    }
}

fn one_module() -> AnalysisData {
    AnalysisData {
        modules: vec![ModuleCouplingRecord {
            module_name: "modA".into(),
            afferent: 1,
            efferent: 9,
            instability: 0.9,
            incoming: vec![],
            outgoing: vec![],
            suppressed: false,
            warning: true,
        }],
        ..Default::default()
    }
}

fn nonempty(v: &serde_json::Value, key: &str) -> bool {
    !v[key].as_array().unwrap_or(&vec![]).is_empty()
}

#[test]
fn json_envelope_contains_every_populated_section() {
    let v = json_value(&analysis_with(rich_findings(), one_module()));
    for key in [
        "dead_code",
        "wildcard_warnings",
        "boilerplate",
        "duplicates",
        "fragments",
        "tq_warnings",
        "structural_warnings",
        "architecture_findings",
    ] {
        assert!(nonempty(&v, key), "section {key} missing: {v}");
    }
    assert!(nonempty(&v["srp"], "param_warnings"), "srp param: {v}");
    // Both the SRP-side (build_srp) and Coupling-side structural rows reach the
    // envelope — pins each chunk's `structural` field against deletion.
    let structural = v["structural_warnings"].to_string();
    assert!(
        structural.contains("SLM"),
        "srp structural (build_srp field): {v}"
    );
    assert!(structural.contains("DEH"), "coupling structural: {v}");
    let coupling = &v["coupling"];
    for key in ["modules", "cycles", "sdp_violations"] {
        assert!(nonempty(coupling, key), "coupling.{key} missing: {v}");
    }
    // The cycle carries its real module names — pins build_cycles against
    // `vec![vec![]]`/`vec![vec!["xyzzy"]]` body replacements.
    assert_eq!(
        coupling["cycles"][0][0], "a",
        "cycle modules preserved: {v}"
    );
    assert_eq!(coupling["cycles"][0][1], "b", "{v}");
}

#[test]
fn coupling_section_present_with_only_a_cycle() {
    // `compose` keeps the coupling section when ANY of modules/cycles is
    // populated. A cycle-only analysis (no modules) pins the
    // `modules.is_empty() && cycles.is_empty()` guard against `||`.
    let findings = AnalysisFindings {
        coupling: vec![cpl(CouplingFindingDetails::Cycle {
            modules: vec!["a".into()],
        })],
        ..Default::default()
    };
    let v = json_value(&analysis_with(findings, AnalysisData::default()));
    assert!(
        !v["coupling"].is_null(),
        "cycle-only keeps coupling section: {v}"
    );
}

#[test]
fn srp_section_present_with_only_a_struct_warning() {
    // Same for SRP: a struct-only analysis keeps the section (pins the
    // `srp_struct.is_empty() && srp_module.is_empty() && srp_param.is_empty()`
    // guard against `||`).
    let findings = AnalysisFindings {
        srp: vec![srp(
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
        )],
        ..Default::default()
    };
    let v = json_value(&analysis_with(findings, AnalysisData::default()));
    assert!(!v["srp"].is_null(), "struct-only keeps srp section: {v}");
}

#[test]
fn suppressed_findings_excluded_from_json() {
    // Marking every finding suppressed empties the suppressible arrays — pins the
    // `!suppressed` filters in build_dead_code/fragments/wildcards/boilerplate/
    // sdp_violations/structural.
    let mut f = rich_findings();
    f.dry.iter_mut().for_each(|d| d.common.suppressed = true);
    f.coupling
        .iter_mut()
        .for_each(|c| c.common.suppressed = true);
    f.srp.iter_mut().for_each(|s| s.common.suppressed = true);
    // A module keeps the coupling section present so sdp_violations is an
    // (empty) array rather than the section collapsing to null.
    let v = json_value(&analysis_with(f, one_module()));
    for key in ["dead_code", "fragments", "wildcard_warnings", "boilerplate"] {
        assert!(
            !nonempty(&v, key),
            "{key} must be empty when suppressed: {v}"
        );
    }
    assert!(
        !nonempty(&v, "structural_warnings"),
        "structural suppressed: {v}"
    );
    let sdp = &v["coupling"]["sdp_violations"];
    assert!(
        sdp.is_null() || sdp.as_array().unwrap().is_empty(),
        "sdp suppressed: {v}"
    );
}
