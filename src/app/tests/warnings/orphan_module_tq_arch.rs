use super::*;

#[test]
fn srp_module_marker_anywhere_in_file_is_not_orphan() {
    // SRP module warnings are suppressed file-globally by
    // `mark_srp_suppressions` — any qual:allow(srp) anywhere in the
    // file matches. The orphan checker must not require line
    // proximity for module-level SRP findings.
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/big.rs".to_string(),
        vec![Suppression {
            line: 500,
            dimensions: vec![crate::findings::Dimension::Srp],
            reason: None,
            target: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis
        .findings
        .srp
        .push(make_srp_module_finding("src/big.rs"));
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &std::collections::HashMap::new(),
        &analysis,
        &Config::default(),
    );
    assert!(
        orphans.is_empty(),
        "SRP module marker at any line must match the file-global module finding, got: {orphans:?}"
    );
}

#[test]
fn tq_marker_within_5_line_window_is_not_orphan() {
    // TQ suppressions use a 5-line window (mark_tq_suppressions).
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![Suppression {
            line: 10,
            dimensions: vec![crate::findings::Dimension::TestQuality],
            reason: None,
            target: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis.findings.test_quality.push(make_tq_finding(
        "src/foo.rs",
        15,
        crate::domain::findings::TqFindingKind::NoAssertion,
    ));
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &std::collections::HashMap::new(),
        &analysis,
        &Config::default(),
    );
    assert!(
        orphans.is_empty(),
        "TQ marker within 5-line window must not be orphan, got: {orphans:?}"
    );
}

#[test]
fn architecture_marker_only_matches_findings_in_window() {
    // Architecture suppressions must be scoped to the marker's
    // annotation window, not the whole file. A `qual:allow(architecture)`
    // for one helper must not silence unrelated layer / forbidden /
    // call-parity findings elsewhere in the same file, and the orphan
    // checker must report a stale marker whose only architecture
    // finding lives outside the window.
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![Suppression {
            line: 1,
            dimensions: vec![crate::findings::Dimension::Architecture],
            reason: None,
            target: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis
        .findings
        .architecture
        .push(make_architecture_finding("src/foo.rs", 500));
    let mut config = Config::default();
    config.architecture.enabled = true;
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &std::collections::HashMap::new(),
        &analysis,
        &config,
    );
    assert!(
        !orphans.is_empty(),
        "Architecture marker at line 1 with only a finding at line 500 must \
         be reported as orphan (window-scoped, not file-scoped); got: {orphans:?}"
    );
}

#[test]
fn complexity_marker_without_any_overshoot_is_orphan() {
    // Sanity: if a marker truly has no target — all complexity metrics
    // are within limits — it IS orphan.
    use crate::adapters::analyzers::iosp::ComplexityMetrics;
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/x.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![crate::findings::Dimension::Complexity],
            reason: None,
            target: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis.results = vec![make_fa_with_complexity(
        "src/x.rs",
        6,
        ComplexityMetrics {
            cognitive_complexity: 1,
            cyclomatic_complexity: 1,
            function_lines: 5,
            ..Default::default()
        },
    )];
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &std::collections::HashMap::new(),
        &analysis,
        &Config::default(),
    );
    assert_eq!(
        orphans.len(),
        1,
        "marker with no over-threshold target must be orphan"
    );
}
