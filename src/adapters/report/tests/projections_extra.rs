//! Tests for the shared Coupling / SRP / TQ projections (`split_*_findings`,
//! `project_tq_rows`). Each pins the per-kind match arms, the `!suppressed`
//! filters, and the detail builders against no-op / arm-deletion mutations.
use crate::adapters::report::projections::coupling::split_coupling_findings;
use crate::adapters::report::projections::srp::split_srp_findings;
use crate::adapters::report::projections::tq::project_tq_rows;
use crate::domain::findings::{
    CouplingFinding, CouplingFindingDetails, CouplingFindingKind, SrpFinding, SrpFindingDetails,
    SrpFindingKind, TqFinding, TqFindingKind,
};
use crate::domain::{Dimension, Finding, Severity};

fn common(dim: Dimension, line: usize, suppressed: bool) -> Finding {
    Finding {
        file: "lib.rs".into(),
        line,
        column: 0,
        dimension: dim,
        rule_id: "r".into(),
        message: "m".into(),
        severity: Severity::Medium,
        suppressed,
    }
}

fn cpl(
    kind: CouplingFindingKind,
    details: CouplingFindingDetails,
    suppressed: bool,
) -> CouplingFinding {
    CouplingFinding {
        common: common(Dimension::Coupling, 1, suppressed),
        kind,
        details,
    }
}

fn sdp(from: &str, suppressed: bool) -> CouplingFinding {
    cpl(
        CouplingFindingKind::SdpViolation,
        CouplingFindingDetails::SdpViolation {
            from_module: from.into(),
            to_module: "dep".into(),
            from_instability: 0.2,
            to_instability: 0.8,
        },
        suppressed,
    )
}

fn structural(code: &str, suppressed: bool) -> CouplingFinding {
    cpl(
        CouplingFindingKind::Structural,
        CouplingFindingDetails::Structural {
            item_name: "I".into(),
            code: code.into(),
            detail: "d".into(),
        },
        suppressed,
    )
}

#[test]
fn coupling_split_buckets_cycles_always_sdp_and_structural_filtered() {
    let findings = vec![
        cpl(
            CouplingFindingKind::Cycle,
            CouplingFindingDetails::Cycle {
                modules: vec!["a".into(), "b".into()],
            },
            false,
        ),
        sdp("live", false),
        sdp("quiet", true),
        structural("OI", false),
        structural("SIT", true),
    ];
    let b = split_coupling_findings(&findings);
    assert_eq!(
        b.cycle_paths.len(),
        1,
        "cycle bucket (arm + always-included)"
    );
    assert_eq!(
        b.sdp_violations.len(),
        1,
        "suppressed SDP filtered (`!suppressed`)"
    );
    assert_eq!(b.sdp_violations[0].from, "live");
    assert_eq!(b.structural_rows.len(), 1);
    assert_eq!(b.structural_rows[0].code, "OI");
}

fn srp(kind: SrpFindingKind, details: SrpFindingDetails) -> SrpFinding {
    SrpFinding {
        common: common(Dimension::Srp, 1, false),
        kind,
        details,
    }
}

#[test]
fn srp_split_buckets_each_kind_and_drops_suppressed() {
    let struct_f = srp(
        SrpFindingKind::StructCohesion,
        SrpFindingDetails::StructCohesion {
            struct_name: "Big".into(),
            lcom4: 4,
            field_count: 9,
            method_count: 7,
            fan_out: 2,
            composite_score: 0.5,
            clusters: vec![],
        },
    );
    let module_f = srp(
        SrpFindingKind::ModuleLength,
        SrpFindingDetails::ModuleLength {
            module: "m".into(),
            production_lines: 900,
            independent_clusters: 2,
            cluster_names: vec![],
            length_score: 0.0,
        },
    );
    let param_f = srp(
        SrpFindingKind::ParameterCount,
        SrpFindingDetails::ParameterCount {
            function_name: "f".into(),
            parameter_count: 7,
        },
    );
    let structural_f = srp(
        SrpFindingKind::Structural,
        SrpFindingDetails::Structural {
            item_name: "Foo::bar".into(),
            code: "SLM".into(),
            detail: "selfless".into(),
        },
    );
    let mut suppressed_struct = struct_f.clone();
    suppressed_struct.common.suppressed = true;
    let b = split_srp_findings(&[struct_f, module_f, param_f, structural_f, suppressed_struct]);
    assert_eq!(b.struct_warnings.len(), 1, "suppressed struct dropped");
    assert_eq!(b.struct_warnings[0].struct_name, "Big");
    assert_eq!(b.module_warnings.len(), 1);
    assert_eq!(b.module_warnings[0].module, "m");
    assert_eq!(b.param_warnings.len(), 1);
    assert_eq!(b.param_warnings[0].function_name, "f");
    assert_eq!(b.structural_rows.len(), 1, "structural arm projected");
    assert_eq!(b.structural_rows[0].code, "SLM");
}

fn tq(
    line: usize,
    kind: TqFindingKind,
    name: &str,
    uncovered: Option<Vec<(String, usize)>>,
) -> TqFinding {
    TqFinding {
        common: common(Dimension::TestQuality, line, false),
        kind,
        function_name: name.into(),
        uncovered_lines: uncovered,
        coverage: crate::domain::findings::CoverageEvidence::NotApplicable,
    }
}

#[test]
fn tq_rows_drop_suppressed_and_untested_logic_lists_uncovered() {
    let logic = tq(
        1,
        TqFindingKind::UntestedLogic,
        "t1",
        Some(vec![("if".into(), 5)]),
    );
    let plain = tq(2, TqFindingKind::NoAssertion, "t2", None);
    let mut suppressed = plain.clone();
    suppressed.common.suppressed = true;
    let rows = project_tq_rows(&[logic, plain, suppressed]);
    assert_eq!(rows.len(), 2, "suppressed dropped (`!suppressed`)");
    let lr = rows.iter().find(|r| r.function_name == "t1").unwrap();
    assert_eq!(lr.detail, "if at line 5", "UntestedLogic detail text");
    let pr = rows.iter().find(|r| r.function_name == "t2").unwrap();
    assert!(pr.detail.is_empty(), "non-logic kinds have no detail");
}
