//! Per-dimension warning/violation counters in `app::metrics`. Each fixture
//! puts a metric/warning exactly at a threshold (or splits suppressed vs not)
//! so the comparison / `||` / `!` mutations in the counting logic flip the
//! asserted total.
use super::*;
use crate::adapters::analyzers::coupling::{CouplingAnalysis, CouplingMetrics, ModuleGraph};
use crate::app::coupling_suppressions::ModuleCouplingSuppressions;
use crate::config::sections::CouplingConfig;

fn coupling_config() -> CouplingConfig {
    CouplingConfig {
        max_instability: 0.5,
        max_fan_in: 5,
        max_fan_out: 5,
        ..CouplingConfig::default()
    }
}

fn metric(afferent: usize, efferent: usize, instability: f64) -> CouplingMetrics {
    CouplingMetrics {
        module_name: "m".to_string(),
        afferent,
        efferent,
        instability,
        incoming: vec![],
        outgoing: vec![],
        suppressed: false,
        warning: false,
    }
}

fn coupling_warnings(metrics: Vec<CouplingMetrics>) -> usize {
    let mut analysis = CouplingAnalysis {
        metrics,
        cycles: vec![],
        sdp_violations: vec![],
        graph: ModuleGraph::default(),
    };
    let config = coupling_config();
    let mut summary = Summary::from_results(&[]);
    count_coupling_warnings(
        Some(&mut analysis),
        &config,
        &ModuleCouplingSuppressions::build(&std::collections::HashMap::new()),
        &mut summary,
    );
    summary.coupling_warnings
}

#[test]
fn coupling_afferent_exactly_at_threshold_does_not_warn() {
    // afferent == max_fan_in (5) is NOT over the threshold. Guards the
    // `m.afferent > config.max_fan_in` comparison (`>` vs `>=`/`==`).
    assert_eq!(coupling_warnings(vec![metric(5, 0, 0.0)]), 0);
}

#[test]
fn coupling_afferent_above_threshold_warns() {
    // afferent above the cap warns even though efferent is below it — guards the
    // `|| m.efferent > max_fan_out` disjunction against becoming `&&`.
    assert_eq!(coupling_warnings(vec![metric(6, 0, 0.0)]), 1);
}

#[test]
fn coupling_efferent_exactly_at_threshold_does_not_warn() {
    // efferent == max_fan_out (5) is NOT over the threshold.
    assert_eq!(coupling_warnings(vec![metric(0, 5, 0.0)]), 0);
}

#[test]
fn coupling_efferent_above_threshold_warns() {
    assert_eq!(coupling_warnings(vec![metric(0, 6, 0.0)]), 1);
}

#[test]
fn coupling_instability_exactly_at_threshold_does_not_warn() {
    // instability == max_instability (0.5) is NOT over the threshold. Guards the
    // `m.instability > config.max_instability` comparison.
    assert_eq!(coupling_warnings(vec![metric(1, 0, 0.5)]), 0);
}

#[test]
fn coupling_instability_above_threshold_warns() {
    assert_eq!(coupling_warnings(vec![metric(1, 0, 0.6)]), 1);
}

#[test]
fn compute_coupling_returns_some_when_enabled() {
    // With coupling enabled (default), `compute_coupling` returns Some — guards
    // the `!config.coupling.enabled` early-return.
    let analysis = compute_coupling(&[], &crate::config::Config::default());
    assert!(analysis.is_some(), "coupling analysis runs when enabled");
}

#[test]
fn count_srp_warnings_counts_unsuppressed_struct_warnings() {
    // Guards `count_srp_warnings` against being a no-op: one unsuppressed struct
    // warning must surface as `summary.srp_struct_warnings == 1`.
    let mut srp = make_srp();
    srp.struct_warnings
        .push(crate::adapters::analyzers::srp::SrpWarning {
            struct_name: "S".to_string(),
            file: "test.rs".to_string(),
            line: 1,
            lcom4: 2,
            field_count: 3,
            method_count: 4,
            fan_out: 1,
            composite_score: 0.9,
            clusters: vec![],
            suppressed: false,
        });
    let mut summary = Summary::from_results(&[]);
    count_srp_warnings(Some(&srp), &mut summary);
    assert_eq!(summary.srp_struct_warnings, 1);
}

#[test]
fn count_sdp_violations_excludes_suppressed_with_distinct_counts() {
    // Two suppressed + one unsuppressed → exactly 1 counted. Distinct counts
    // (1 vs 2) so deleting the `!` (counting suppressed instead) is observable.
    use crate::adapters::analyzers::coupling::sdp::SdpViolation;
    let sdp = |suppressed: bool| SdpViolation {
        from_module: "a".into(),
        to_module: "b".into(),
        from_instability: 0.2,
        to_instability: 0.8,
        suppressed,
    };
    let analysis = CouplingAnalysis {
        metrics: vec![],
        cycles: vec![],
        sdp_violations: vec![sdp(true), sdp(true), sdp(false)],
        graph: ModuleGraph::default(),
    };
    let config = CouplingConfig::default();
    let mut summary = Summary::from_results(&[]);
    count_sdp_violations(Some(&analysis), &config, &mut summary);
    assert_eq!(
        summary.sdp_violations, 1,
        "only the one unsuppressed violation counts"
    );
}

// ── DRY finding counts + wildcard suppression window ──────────

use crate::adapters::analyzers::dry::wildcards::WildcardImportWarning;
use crate::findings::Dimension;
use std::collections::HashMap;

fn sups_at(line: usize, dim: Dimension) -> HashMap<String, Vec<Suppression>> {
    [(
        "test.rs".to_string(),
        vec![Suppression {
            line,
            dimensions: vec![dim],
            reason: None,
            target: None,
        }],
    )]
    .into()
}

fn wildcard_suppressed(w_line: usize, sup_line: usize, dim: Dimension) -> bool {
    let mut warnings = vec![WildcardImportWarning {
        file: "test.rs".to_string(),
        line: w_line,
        module_path: "crate::x::*".to_string(),
        suppressed: false,
    }];
    mark_wildcard_suppressions(&mut warnings, &sups_at(sup_line, dim));
    warnings[0].suppressed
}

#[test]
fn wildcard_suppression_same_line_suppresses() {
    // sup.line == w.line is within the window; guards the `sup.line <= w.line`
    // bound (`<=` vs `>`).
    assert!(wildcard_suppressed(5, 5, Dimension::Dry));
}

#[test]
fn wildcard_suppression_one_line_above_suppresses() {
    // diff 1 == WILDCARD window; guards the `w.line - sup.line <= window` bound
    // and the subtraction (a `/` would give 2/1=2 > 1, a `+` 2+1=3 > 1).
    assert!(wildcard_suppressed(2, 1, Dimension::Dry));
}

#[test]
fn wildcard_suppression_two_lines_above_does_not_suppress() {
    // diff 2 > window 1 → not suppressed.
    assert!(!wildcard_suppressed(3, 1, Dimension::Dry));
}

#[test]
fn wildcard_suppression_below_warning_does_not_suppress() {
    // A suppression *below* the warning must not apply; guards the first `&&`
    // (an `||` would evaluate `w.line - sup.line` and underflow, or wrongly
    // suppress).
    assert!(!wildcard_suppressed(2, 5, Dimension::Dry));
}

#[test]
fn wildcard_suppression_wrong_dimension_does_not_suppress() {
    // In-window but covering a different dimension → not suppressed; guards the
    // `&& sup.covers(dry_dim)` conjunction.
    assert!(!wildcard_suppressed(5, 5, Dimension::Complexity));
}

fn one_entry_dup(suppressed: bool) -> DuplicateGroup {
    DuplicateGroup {
        entries: vec![DuplicateEntry {
            name: "a".into(),
            qualified_name: "a".into(),
            file: "test.rs".into(),
            line: 1,
        }],
        kind: DuplicateKind::Exact,
        suppressed,
    }
}

fn one_entry_frag(suppressed: bool) -> FragmentGroup {
    FragmentGroup {
        entries: vec![FragmentEntry {
            function_name: "f".into(),
            qualified_name: "f".into(),
            file: "test.rs".into(),
            start_line: 1,
            end_line: 3,
        }],
        statement_count: 3,
        suppressed,
    }
}

fn one_bp(suppressed: bool) -> BoilerplateFind {
    BoilerplateFind {
        pattern_id: "BP-001".into(),
        file: "test.rs".into(),
        line: 1,
        struct_name: None,
        description: String::new(),
        suggestion: String::new(),
        suppressed,
    }
}

fn one_wildcard(suppressed: bool) -> WildcardImportWarning {
    WildcardImportWarning {
        file: "test.rs".into(),
        line: 1,
        module_path: "crate::x::*".into(),
        suppressed,
    }
}

fn one_entry_rep(suppressed: bool) -> RepeatedMatchGroup {
    RepeatedMatchGroup {
        enum_name: "E".into(),
        entries: vec![RepeatedMatchEntry {
            file: "test.rs".into(),
            line: 1,
            function_name: "h".into(),
            arm_count: 4,
        }],
        suppressed,
    }
}

#[test]
fn count_dry_findings_counts_only_unsuppressed() {
    // Each DRY vector has 2 unsuppressed + 1 suppressed; the summary must report
    // 2 (not 1, not 0). Guards the `!g.suppressed` filters and the function
    // against being a no-op.
    let dry = DryResults {
        duplicates: vec![
            one_entry_dup(false),
            one_entry_dup(false),
            one_entry_dup(true),
        ],
        dead_code: vec![],
        fragments: vec![
            one_entry_frag(false),
            one_entry_frag(false),
            one_entry_frag(true),
        ],
        boilerplate: vec![one_bp(false), one_bp(false), one_bp(true)],
        wildcard_warnings: vec![one_wildcard(false), one_wildcard(false), one_wildcard(true)],
    };
    let repeated = vec![
        one_entry_rep(false),
        one_entry_rep(false),
        one_entry_rep(true),
    ];
    let mut summary = Summary::from_results(&[]);
    count_dry_findings(&dry, &repeated, &mut summary);
    assert_eq!(summary.duplicate_groups, 2, "duplicates");
    assert_eq!(summary.fragment_groups, 2, "fragments");
    assert_eq!(summary.boilerplate_warnings, 2, "boilerplate");
    assert_eq!(summary.wildcard_import_warnings, 2, "wildcards");
    assert_eq!(summary.repeated_match_groups, 2, "repeated matches");
}
