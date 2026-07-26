//! `app::projection` for the secondary passes (coupling, SRP, structural,
//! data, DRY). Pins the filter predicates (`warning && !suppressed`, the
//! `dimension ==` selectors), the structural rule-id prefix (`srp`/`coupling`
//! match arms), and the per-group projection helpers against `-> vec![]`.
use crate::adapters::analyzers::coupling::{CouplingAnalysis, CouplingMetrics};
use crate::adapters::analyzers::dry::fragments::{FragmentEntry, FragmentGroup};
use crate::adapters::analyzers::dry::match_patterns::{RepeatedMatchEntry, RepeatedMatchGroup};
use crate::adapters::analyzers::structural::{
    StructuralAnalysis, StructuralWarning, StructuralWarningKind,
};
use crate::app::projection::{project_coupling, project_data, project_dry, project_srp};
use crate::app::secondary::SecondaryResults;
use crate::domain::findings::{CouplingFindingKind, DryFindingKind, SrpFindingKind};
use crate::findings::Dimension;

fn metric(name: &str, warning: bool, suppressed: bool) -> CouplingMetrics {
    CouplingMetrics {
        module_name: name.into(),
        afferent: 1,
        efferent: 9,
        instability: 0.9,
        incoming: vec![],
        outgoing: vec![],
        suppressed,
        warning,
    }
}

fn coupling_with(metrics: Vec<CouplingMetrics>) -> CouplingAnalysis {
    CouplingAnalysis {
        metrics,
        cycles: vec![],
        sdp_violations: vec![],
        graph: Default::default(),
    }
}

fn structural_warning(dim: Dimension, kind: StructuralWarningKind) -> StructuralAnalysis {
    StructuralAnalysis {
        warnings: vec![StructuralWarning {
            file: "src/x.rs".into(),
            line: 10,
            name: "Foo".into(),
            kind,
            dimension: dim,
            suppressed: false,
        }],
    }
}

#[test]
fn project_coupling_threshold_filter_is_warning_and_not_suppressed() {
    // Only a metric that is `warning && !suppressed` becomes a ThresholdExceeded
    // finding. A second, non-warning metric must be dropped — pinning the
    // `warning && !suppressed` filter (`&&`→`||` would admit it, deleting `!`
    // would reject the live one).
    let coupling = coupling_with(vec![
        metric("live", true, false),
        metric("quiet", false, false),
    ]);
    let breaches = project_coupling(Some(&coupling), None)
        .into_iter()
        .filter(|f| matches!(f.kind, CouplingFindingKind::ThresholdExceeded))
        .count();
    assert_eq!(
        breaches, 1,
        "only the warning, unsuppressed metric breaches"
    );
}

#[test]
fn project_coupling_takes_only_coupling_dimension_structural_warnings() {
    // A structural warning carrying `dimension == Coupling` projects into a
    // Structural coupling finding with a `coupling/structural/...` rule id.
    // Pins the `w.dimension == Coupling` selector and the `Coupling` match arm
    // in `structural_pieces`.
    let structural = structural_warning(
        Dimension::Coupling,
        StructuralWarningKind::DowncastEscapeHatch,
    );
    let findings = project_coupling(None, Some(&structural));
    let s = findings
        .iter()
        .find(|f| matches!(f.kind, CouplingFindingKind::Structural))
        .expect("structural coupling finding");
    assert!(
        s.common.rule_id.starts_with("coupling/structural/"),
        "rule_id keyed by the Coupling arm, got {}",
        s.common.rule_id
    );
}

#[test]
fn project_srp_takes_only_srp_dimension_structural_warnings() {
    // Mirror for SRP: a `dimension == Srp` structural warning projects into a
    // Structural SRP finding with a `srp/structural/...` rule id. Pins the
    // `w.dimension == Srp` selector and the `Srp` match arm.
    let structural = structural_warning(Dimension::Srp, StructuralWarningKind::SelflessMethod);
    let findings = project_srp(None, Some(&structural));
    let s = findings
        .iter()
        .find(|f| matches!(f.kind, SrpFindingKind::Structural))
        .expect("structural srp finding");
    assert!(
        s.common.rule_id.starts_with("srp/structural/"),
        "rule_id keyed by the Srp arm, got {}",
        s.common.rule_id
    );
}

#[test]
fn project_data_emits_one_module_record_per_metric() {
    // Guards `project_modules` against `-> vec![]`.
    let coupling = coupling_with(vec![metric("m", true, false)]);
    let data = project_data(&[], Some(&coupling));
    assert_eq!(data.modules.len(), 1, "one ModuleCouplingRecord per metric");
}

fn empty_secondary() -> SecondaryResults {
    SecondaryResults {
        stale_markers: Vec::new(),
        coupling: None,
        duplicates: vec![],
        dead_code: vec![],
        fragments: vec![],
        boilerplate: vec![],
        wildcard_warnings: vec![],
        repeated_matches: vec![],
        srp: None,
        tq: None,
        structural: None,
    }
}

#[test]
fn project_dry_emits_fragment_and_repeated_match_findings() {
    // Guards `project_fragment_group` and `project_repeated_match_group` against
    // `-> vec![]`. A fragment group with two entries and a repeated-match group
    // with two entries each project one finding per entry.
    let mut secondary = empty_secondary();
    secondary.fragments = vec![FragmentGroup {
        entries: vec![
            FragmentEntry {
                function_name: "a".into(),
                qualified_name: "m::a".into(),
                file: "src/x.rs".into(),
                start_line: 1,
                end_line: 8,
            },
            FragmentEntry {
                function_name: "b".into(),
                qualified_name: "m::b".into(),
                file: "src/x.rs".into(),
                start_line: 20,
                end_line: 27,
            },
        ],
        statement_count: 6,
        suppressed: false,
    }];
    secondary.repeated_matches = vec![RepeatedMatchGroup {
        enum_name: "E".into(),
        entries: vec![
            RepeatedMatchEntry {
                file: "src/x.rs".into(),
                line: 3,
                function_name: "f".into(),
                arm_count: 4,
            },
            RepeatedMatchEntry {
                file: "src/x.rs".into(),
                line: 40,
                function_name: "g".into(),
                arm_count: 4,
            },
        ],
        suppressed: false,
    }];
    let findings = project_dry(&secondary);
    let fragments = findings
        .iter()
        .filter(|f| matches!(f.kind, DryFindingKind::Fragment))
        .count();
    let repeated = findings
        .iter()
        .filter(|f| matches!(f.kind, DryFindingKind::RepeatedMatch))
        .count();
    assert_eq!(fragments, 2, "one fragment finding per entry");
    assert_eq!(repeated, 2, "one repeated-match finding per entry");
}
