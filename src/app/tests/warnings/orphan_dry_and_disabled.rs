use super::*;

#[test]
fn dry_marker_orphan_depends_on_finding_kind_and_window() {
    // `qual:allow(dry)` matches a line-anchored DRY finding only within the
    // relevant window. DEAD_CODE isn't dry-suppressible at all → orphan; a
    // wildcard finding has a 1-line window, so a marker 2 lines above is
    // orphan but 1 line above is not. (label, sup_line, finding, expected)
    type DryMaker = fn(&str, usize) -> crate::domain::findings::DryFinding;
    let cases: &[(&str, usize, DryMaker, usize)] = &[
        (
            "dead_code is not dry-suppressible → orphan",
            5,
            make_dry_dead_code_finding,
            1,
        ),
        (
            "two lines above a wildcard (window 1) → orphan",
            5,
            make_dry_wildcard_finding,
            1,
        ),
        (
            "one line above a wildcard (window 1) → not orphan",
            6,
            make_dry_wildcard_finding,
            0,
        ),
    ];
    for (label, sup_line, finding, expected) in cases {
        let mut sups = HashMap::new();
        sups.insert(
            "src/foo.rs".to_string(),
            vec![crate::findings::Suppression {
                line: *sup_line,
                dimensions: vec![crate::findings::Dimension::Dry],
                reason: None,
            }],
        );
        let mut analysis = empty_analysis();
        analysis.findings.dry.push(finding("src/foo.rs", 7));
        let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
            &sups,
            &std::collections::HashMap::new(),
            &analysis,
            &Config::default(),
        );
        assert_eq!(orphans.len(), *expected, "case {label}: got {orphans:?}");
    }
}

#[test]
fn complexity_marker_is_orphan_when_complexity_dimension_disabled() {
    // Config disables the complexity dimension entirely. A
    // `qual:allow(complexity)` marker can't suppress what doesn't
    // run, so it IS orphan even on a function with over-threshold
    // metrics.
    use crate::adapters::analyzers::iosp::ComplexityMetrics;
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/x.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![crate::findings::Dimension::Complexity],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis.results = vec![make_fa_with_complexity(
        "src/x.rs",
        6,
        ComplexityMetrics {
            cognitive_complexity: 99,
            ..Default::default()
        },
    )];
    let mut config = Config::default();
    config.complexity.enabled = false;
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &std::collections::HashMap::new(),
        &analysis,
        &config,
    );
    assert_eq!(
        orphans.len(),
        1,
        "complexity marker with dimension disabled must be orphan"
    );
}

#[test]
fn srp_marker_is_orphan_when_srp_dimension_disabled() {
    // Same pattern for SRP: disable the dimension, a qual:allow(srp)
    // can't suppress anything → orphan even if a SrpWarning is in
    // the analysis struct (stale from a previous run, or leftover).
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![Suppression {
            line: 2,
            dimensions: vec![crate::findings::Dimension::Srp],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis
        .findings
        .srp
        .push(make_srp_struct_finding("src/foo.rs", 7));
    let mut config = Config::default();
    config.srp.enabled = false;
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &std::collections::HashMap::new(),
        &analysis,
        &config,
    );
    assert_eq!(
        orphans.len(),
        1,
        "SRP marker with dimension disabled must be orphan"
    );
}

#[test]
fn srp_marker_within_5_line_window_is_not_orphan() {
    // SRP struct + structural-binary suppressions both use a 5-line window
    // (wider than ANNOTATION_WINDOW=3) because `#[derive(...)]` attributes
    // can push the marker further from the item. A `qual:allow(srp)` marker
    // matching an in-window SRP finding must not be flagged as orphan.
    // (label, sup_line, finding_line, finding maker)
    type SrpMaker = fn(&str, usize) -> crate::domain::findings::SrpFinding;
    let cases: &[(&str, usize, usize, SrpMaker)] = &[
        (
            "struct finding at diff=5",
            2,
            7,
            make_srp_struct_finding as SrpMaker,
        ),
        (
            "structural-binary finding at diff=5",
            10,
            15,
            make_structural_srp_finding,
        ),
    ];
    for (label, sup_line, finding_line, maker) in cases {
        let mut sups = HashMap::new();
        sups.insert(
            "src/foo.rs".to_string(),
            vec![crate::findings::Suppression {
                line: *sup_line,
                dimensions: vec![crate::findings::Dimension::Srp],
                reason: None,
            }],
        );
        let mut analysis = empty_analysis();
        analysis
            .findings
            .srp
            .push(maker("src/foo.rs", *finding_line));
        let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
            &sups,
            &std::collections::HashMap::new(),
            &analysis,
            &Config::default(),
        );
        assert!(
            orphans.is_empty(),
            "case {label}: in-window SRP marker must not be orphan, got {orphans:?}"
        );
    }
}
