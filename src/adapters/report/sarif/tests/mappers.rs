//! Tests for the SARIF rule-id mappers (`*_rule`) and the `helpUri` guard.
//! Each finding kind / structural code maps to a fixed rule id; asserting the
//! exact id pins every match arm and the `-> ""`/"xyzzy" body replacements.
use crate::domain::findings::{
    ComplexityFindingKind, CouplingFinding, CouplingFindingDetails, CouplingFindingKind,
    DryFinding, DryFindingDetails, DryFindingKind, SrpFinding, SrpFindingDetails, SrpFindingKind,
    TqFindingKind,
};
use crate::domain::{Dimension, Finding, Severity};
use crate::report::sarif::rules::*;

fn common(dim: Dimension) -> Finding {
    Finding {
        file: "x.rs".into(),
        line: 1,
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

#[test]
fn complexity_rule_maps_each_kind() {
    use ComplexityFindingKind::*;
    let cases = [
        (Cognitive, "CX-001"),
        (Cyclomatic, "CX-002"),
        (MagicNumber, "CX-003"),
        (FunctionLength, "CX-004"),
        (NestingDepth, "CX-005"),
        (Unsafe, "CX-006"),
        (ErrorHandling, "A20"),
    ];
    for (kind, want) in cases {
        assert_eq!(complexity_rule(kind), want, "{kind:?}");
    }
}

#[test]
fn dry_rule_maps_each_kind() {
    let dup = dry(
        DryFindingKind::DuplicateExact,
        DryFindingDetails::DeadCode {
            qualified_name: String::new(),
            suggestion: None,
        },
    );
    assert_eq!(dry_rule(&dup), "DRY-001");
    let dead = dry(
        DryFindingKind::DeadCodeUncalled,
        DryFindingDetails::DeadCode {
            qualified_name: String::new(),
            suggestion: None,
        },
    );
    assert_eq!(dry_rule(&dead), "DRY-002");
    let frag = dry(
        DryFindingKind::Fragment,
        DryFindingDetails::DeadCode {
            qualified_name: String::new(),
            suggestion: None,
        },
    );
    assert_eq!(dry_rule(&frag), "DRY-003");
    let bp = dry(
        DryFindingKind::Boilerplate,
        DryFindingDetails::Boilerplate {
            pattern_id: "BP-007".into(),
            struct_name: None,
            suggestion: String::new(),
        },
    );
    assert_eq!(dry_rule(&bp), "BP-007", "boilerplate uses its pattern_id");
}

#[test]
fn srp_rule_maps_each_kind_and_structural_codes() {
    assert_eq!(
        srp_rule(&srp(
            SrpFindingKind::StructCohesion,
            SrpFindingDetails::ParameterCount {
                function_name: String::new(),
                parameter_count: 0,
            },
        )),
        "SRP-001"
    );
    assert_eq!(
        srp_rule(&srp(
            SrpFindingKind::ModuleLength,
            SrpFindingDetails::ParameterCount {
                function_name: String::new(),
                parameter_count: 0,
            },
        )),
        "SRP-002"
    );
    assert_eq!(
        srp_rule(&srp(
            SrpFindingKind::ParameterCount,
            SrpFindingDetails::ParameterCount {
                function_name: String::new(),
                parameter_count: 0,
            },
        )),
        "SRP-003"
    );
    // Structural → structural_rule(code): each code maps to itself.
    for code in ["BTC", "SLM", "NMS", "OI", "SIT", "DEH", "IET"] {
        let f = srp(
            SrpFindingKind::Structural,
            SrpFindingDetails::Structural {
                item_name: "I".into(),
                code: code.into(),
                detail: "d".into(),
            },
        );
        assert_eq!(srp_rule(&f), code, "structural code {code}");
    }
}

#[test]
fn coupling_rule_maps_each_detail() {
    let cpl = |details| CouplingFinding {
        common: common(Dimension::Coupling),
        kind: CouplingFindingKind::Structural,
        details,
    };
    assert_eq!(
        coupling_rule(&cpl(CouplingFindingDetails::Cycle { modules: vec![] })),
        "CP-001"
    );
    assert_eq!(
        coupling_rule(&cpl(CouplingFindingDetails::SdpViolation {
            from_module: String::new(),
            to_module: String::new(),
            from_instability: 0.0,
            to_instability: 0.0,
        })),
        "CP-002"
    );
    assert_eq!(
        coupling_rule(&cpl(CouplingFindingDetails::ThresholdExceeded {
            module_name: String::new(),
            afferent: 0,
            efferent: 0,
            instability: 0.0,
        })),
        "CP-003"
    );
    assert_eq!(
        coupling_rule(&cpl(CouplingFindingDetails::Structural {
            item_name: String::new(),
            code: "DEH".into(),
            detail: String::new(),
        })),
        "DEH"
    );
}

#[test]
fn tq_rule_maps_kinds_and_help_uri_only_on_iosp() {
    assert_eq!(tq_rule(&TqFindingKind::NoAssertion), "TQ-001");
    // The `id == "iosp/violation"` guard adds a helpUri to exactly that rule.
    let rules = sarif_rules();
    let with_help = |id: &str| {
        rules
            .iter()
            .find(|r| r["id"] == id)
            .map(|r| r.get("helpUri").is_some())
            .unwrap_or(false)
    };
    assert!(with_help("iosp/violation"), "iosp rule carries helpUri");
    assert!(!with_help("CX-001"), "non-iosp rules have no helpUri");
}
