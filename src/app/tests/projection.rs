//! `app::projection` — analysis structs → typed domain findings. The flag/field
//! mutations (`fn → vec![]`, dropped arms, count arithmetic, dimension `==`) are
//! observed by asserting the projected findings.
use crate::adapters::analyzers::iosp::{
    ComplexityHotspot, ComplexityMetrics, FunctionAnalysis, MagicNumberOccurrence,
};
use crate::app::projection::{project_architecture, project_complexity};
use crate::config::Config;
use crate::domain::findings::ComplexityFindingKind;
use crate::domain::{Dimension, Finding};

fn func(metrics: ComplexityMetrics) -> FunctionAnalysis {
    super::warnings::make_func_with_metrics(metrics)
}

#[test]
fn project_complexity_emits_cognitive_finding() {
    // Guards `push_metric_threshold` against being a no-op.
    let mut f = func(ComplexityMetrics {
        cognitive_complexity: 10,
        ..Default::default()
    });
    f.cognitive_warning = true;
    let findings = project_complexity(&[f], &Config::default());
    assert!(findings
        .iter()
        .any(|x| matches!(x.kind, ComplexityFindingKind::Cognitive) && x.metric_value == 10));
}

#[test]
fn project_complexity_error_handling_sums_all_counts() {
    // metric_value = unwrap + expect + panic + todo = 4; guards `error_handling_count`
    // (the `+` arithmetic and the `-> 0`/`-> 1` replacements).
    let mut f = func(ComplexityMetrics {
        unwrap_count: 1,
        expect_count: 1,
        panic_count: 1,
        todo_count: 1,
        ..Default::default()
    });
    f.error_handling_warning = true;
    let findings = project_complexity(&[f], &Config::default());
    let eh = findings
        .iter()
        .find(|x| matches!(x.kind, ComplexityFindingKind::ErrorHandling))
        .expect("error-handling finding");
    assert_eq!(eh.metric_value, 4, "1 + 1 + 1 + 1");
}

#[test]
fn project_complexity_nesting_carries_hotspot() {
    // Guards `nesting_hotspot` against returning None.
    let mut f = func(ComplexityMetrics {
        max_nesting: 5,
        hotspots: vec![ComplexityHotspot {
            line: 3,
            nesting_depth: 5,
            construct: "if".to_string(),
        }],
        ..Default::default()
    });
    f.nesting_depth_warning = true;
    let findings = project_complexity(&[f], &Config::default());
    let nd = findings
        .iter()
        .find(|x| matches!(x.kind, ComplexityFindingKind::NestingDepth))
        .expect("nesting finding");
    assert!(nd.hotspot.is_some(), "nesting finding carries its hotspot");
}

#[test]
fn project_complexity_emits_magic_number_findings() {
    // Guards `push_magic_number_findings` against being a no-op.
    let f = func(ComplexityMetrics {
        magic_numbers: vec![MagicNumberOccurrence {
            line: 1,
            value: "42".to_string(),
        }],
        ..Default::default()
    });
    let findings = project_complexity(&[f], &Config::default());
    assert!(findings
        .iter()
        .any(|x| matches!(x.kind, ComplexityFindingKind::MagicNumber)));
}

#[test]
fn project_architecture_wraps_each_legacy_finding() {
    // Guards `project_architecture` against returning an empty vec.
    let legacy = vec![Finding {
        dimension: Dimension::Architecture,
        rule_id: "architecture/forbidden_edge".to_string(),
        ..Default::default()
    }];
    let findings = project_architecture(&legacy);
    assert_eq!(
        findings.len(),
        1,
        "each legacy finding becomes one ArchitectureFinding"
    );
}
