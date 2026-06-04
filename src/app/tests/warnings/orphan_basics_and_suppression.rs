use super::*;

#[test]
fn orphan_suppression_window_and_dimension_matching() {
    // An orphan is a marker with no matching in-window finding. The marker
    // (line 5) matches an SRP finding within ANNOTATION_WINDOW=3 lines that
    // shares its dimension; a wrong dimension still orphans; an empty-dim
    // marker acts as a wildcard (defensive — the parser no longer emits one).
    // (label, dims, finding_line, expected_orphans)
    use crate::findings::Dimension;
    let cases: &[(&str, &[Dimension], Option<usize>, usize)] = &[
        ("unmatched marker, no finding", &[Dimension::Srp], None, 1),
        ("in-window finding matches", &[Dimension::Srp], Some(8), 0),
        ("dimension mismatch orphans", &[Dimension::Dry], Some(7), 1),
        ("empty-dim acts as wildcard", &[], Some(6), 0),
    ];
    for (label, dims, finding_line, expected) in cases {
        assert_eq!(
            srp_orphan_count(dims, *finding_line),
            *expected,
            "case {label}"
        );
    }
}

/// Flag-style complexity metrics (cognitive / cyclomatic / length / nesting),
/// each over its threshold: `(label, sup_line, fn_line, metrics)`.
fn complexity_flag_cases() -> Vec<(&'static str, usize, usize, ComplexityMetrics)> {
    vec![
        (
            "cognitive",
            5,
            6,
            ComplexityMetrics {
                cognitive_complexity: 99,
                ..Default::default()
            },
        ),
        (
            "cyclomatic",
            5,
            6,
            ComplexityMetrics {
                cyclomatic_complexity: 99,
                ..Default::default()
            },
        ),
        (
            "function length",
            5,
            6,
            ComplexityMetrics {
                function_lines: 200,
                ..Default::default()
            },
        ),
        (
            "nesting",
            5,
            6,
            ComplexityMetrics {
                max_nesting: 10,
                ..Default::default()
            },
        ),
    ]
}

/// The remaining over-threshold metric kinds: unsafe blocks, unwrap count, and
/// a magic number. `(label, sup_line, fn_line, metrics)`.
fn complexity_metric_extra_cases() -> Vec<(&'static str, usize, usize, ComplexityMetrics)> {
    use crate::adapters::analyzers::iosp::MagicNumberOccurrence;
    vec![
        (
            "unsafe block",
            5,
            6,
            ComplexityMetrics {
                unsafe_blocks: 1,
                ..Default::default()
            },
        ),
        (
            "error handling unwrap",
            5,
            6,
            ComplexityMetrics {
                unwrap_count: 3,
                ..Default::default()
            },
        ),
        (
            "magic number",
            10,
            6,
            ComplexityMetrics {
                magic_numbers: vec![MagicNumberOccurrence {
                    line: 12,
                    value: "42".into(),
                }],
                ..Default::default()
            },
        ),
    ]
}

#[test]
fn suppressed_complexity_metric_over_threshold_is_not_orphan() {
    // A `qual:allow(complexity)` marker clears the matching *_warning flag, but
    // the orphan detector reads RAW metrics against config thresholds — when the
    // raw metric still exceeds threshold the marker matches a real (suppressed)
    // finding and must NOT be flagged orphan. Covers every complexity metric kind.
    let mut cases = complexity_flag_cases();
    cases.extend(complexity_metric_extra_cases());
    for (label, sup_line, fn_line, metrics) in &cases {
        let orphans = complexity_sup_orphans(*sup_line, *fn_line, metrics.clone());
        assert!(
            orphans.is_empty(),
            "case {label}: marker clearing a complexity flag must not be orphan, got {orphans:?}"
        );
    }
}

#[test]
fn suppressed_srp_param_over_threshold_is_not_orphan() {
    // A `// qual:allow(srp)` marker on a function with >5 parameters:
    // `apply_parameter_warnings` now records the warning with
    // suppressed=true (it used to filter them out), so the orphan
    // checker finds a matching SRP position.
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/x.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![crate::findings::Dimension::Srp],
            reason: None,
            target: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis
        .findings
        .srp
        .push(make_srp_param_finding("src/x.rs", 6, true));
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &std::collections::HashMap::new(),
        &analysis,
        &Config::default(),
    );
    assert!(
        orphans.is_empty(),
        "SRP param marker must match even on suppressed warnings, got: {orphans:?}"
    );
}

#[test]
fn coupling_marker_is_not_orphan_for_structural_coupling_finding() {
    // Structural binary checks (OI, SIT, DEH, IET) carry
    // `dimension == Coupling` and are line-anchored — a 5-line
    // qual:allow(coupling) window DOES suppress them. The orphan
    // checker must treat coupling-only markers as verifiable when a
    // line-anchored coupling position is available in the file.
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![Suppression {
            line: 10,
            dimensions: vec![crate::findings::Dimension::Coupling],
            reason: None,
            target: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis
        .findings
        .coupling
        .push(make_structural_coupling_finding("src/foo.rs", 12));
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &std::collections::HashMap::new(),
        &analysis,
        &Config::default(),
    );
    assert!(
        orphans.is_empty(),
        "coupling marker for a line-anchored structural finding must not be orphan, got: {orphans:?}"
    );
}

#[test]
fn coupling_only_marker_with_no_line_anchored_finding_is_skipped() {
    // When the file has no line-anchored Coupling position, a
    // coupling-only marker is unverifiable (pure module-level
    // coupling is global). We skip it rather than emit a
    // potentially-false orphan.
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![crate::findings::Dimension::Coupling],
            reason: None,
            target: None,
        }],
    );
    let analysis = empty_analysis();
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &std::collections::HashMap::new(),
        &analysis,
        &Config::default(),
    );
    assert!(
        orphans.is_empty(),
        "coupling-only marker without a line-anchored Coupling finding must be skipped, got: {orphans:?}"
    );
}
