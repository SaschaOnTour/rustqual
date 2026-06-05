//! Internal predicates of the orphan detector (`app::orphan_suppressions::mod`)
//! that the higher-level orphan tests don't pin: the coupling-only
//! `is_verifiable` branch, the DRY enable-guard truth table, architecture
//! in-window matching, the invalid-marker projection, and the `is_test`
//! short-circuits in `push_magic_numbers` / `exceeds_error_handling`.
use super::*;
use crate::adapters::analyzers::iosp::{ComplexityMetrics, MagicNumberOccurrence};
use crate::adapters::suppression::qual_allow::InvalidQualAllow;
use crate::findings::{Dimension, Suppression};

/// One marker on `file` at `line` covering `dims`.
fn marker(file: &str, line: usize, dims: &[Dimension]) -> HashMap<String, Vec<Suppression>> {
    let mut sups = HashMap::new();
    sups.insert(
        file.to_string(),
        vec![Suppression {
            line,
            dimensions: dims.to_vec(),
            reason: None,
            target: None,
        }],
    );
    sups
}

fn run(
    sups: &HashMap<String, Vec<Suppression>>,
    analysis: &crate::report::AnalysisResult,
    config: &Config,
) -> usize {
    crate::app::orphan_suppressions::detect_orphan_suppressions(
        sups,
        &std::collections::HashMap::new(),
        analysis,
        config,
    )
    .len()
}

#[test]
fn coupling_only_marker_is_verifiable_when_a_coupling_finding_exists() {
    // A coupling-only marker is verifiable iff the file has a line-anchored
    // Coupling finding. Here the only coupling finding is out of window, so the
    // marker is a verifiable-but-unmatched orphan. The `p.dim == Coupling`
    // probe flipped to `!=` would make the file look like it has no coupling
    // anchor → marker skipped → 0. Asserting 1 pins the `==`.
    let sups = marker("src/foo.rs", 1, &[Dimension::Coupling]);
    let mut analysis = empty_analysis();
    analysis
        .findings
        .coupling
        .push(make_structural_coupling_finding("src/foo.rs", 500));
    assert_eq!(
        run(&sups, &analysis, &Config::default()),
        1,
        "coupling-only marker with an out-of-window coupling finding is a verifiable orphan"
    );
}

#[test]
fn dry_enable_guard_truth_table() {
    // `collect_dry_positions` returns early only when BOTH duplicates and
    // boilerplate are disabled. A matchable DRY finding sits one line below the
    // marker (wildcard window = 1). (dup_enabled, bp_enabled, expected_orphans):
    // both off → no positions → orphan; either on → position matches → not.
    // Kills `&&`→`||` (the one-on rows) and either `!` deletion (the both-off row).
    let cases = [(false, false, 1), (true, false, 0), (false, true, 0)];
    for (dup, bp, expected) in cases {
        let sups = marker("src/foo.rs", 6, &[Dimension::Dry]);
        let mut analysis = empty_analysis();
        analysis
            .findings
            .dry
            .push(make_dry_wildcard_finding("src/foo.rs", 7));
        let mut config = Config::default();
        config.duplicates.enabled = dup;
        config.boilerplate.enabled = bp;
        config.compile();
        assert_eq!(
            run(&sups, &analysis, &config),
            expected,
            "dup={dup} bp={bp}: dry enable-guard"
        );
    }
}

#[test]
fn architecture_marker_in_window_is_not_orphan() {
    // An architecture finding two lines below the marker (DEFAULT window = 3) is
    // matched → not orphan. A `collect_architecture_positions` that drops the
    // position (body replaced, or the `enabled` guard inverted) would leave the
    // marker unmatched → orphan. Asserting 0 pins the collection.
    let sups = marker("src/foo.rs", 1, &[Dimension::Architecture]);
    let mut analysis = empty_analysis();
    analysis
        .findings
        .architecture
        .push(make_architecture_finding("src/foo.rs", 3));
    let mut config = Config::default();
    config.architecture.enabled = true;
    assert_eq!(
        run(&sups, &analysis, &config),
        0,
        "architecture marker matching an in-window finding must not be orphan"
    );
}

#[test]
fn invalid_marker_surfaces_as_orphan() {
    // A malformed `qual:allow(...)` (typo'd dimension) suppresses nothing but
    // must surface so the author sees the stale marker. Pins
    // `invalid_marker_orphans` against being replaced by an empty vec.
    let invalid: HashMap<String, Vec<(usize, InvalidQualAllow)>> = [(
        "src/foo.rs".to_string(),
        vec![(5, InvalidQualAllow::UnknownDimensions("srp_params".into()))],
    )]
    .into();
    let analysis = empty_analysis();
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &HashMap::new(),
        &invalid,
        &analysis,
        &Config::default(),
    );
    assert_eq!(
        orphans.len(),
        1,
        "invalid marker must surface as one orphan"
    );
    assert_eq!(orphans[0].line, 5);
}

#[test]
fn magic_numbers_skipped_for_test_fn_leaves_marker_orphan() {
    // A magic number on a *test* function is not a finding (magic-number check
    // skips tests), so a `qual:allow(complexity)` marker over it has no target →
    // orphan. The `f.is_test || !detect_magic_numbers` short-circuit flipped to
    // `&&` would push the magic position and wrongly match the marker.
    let fa = make_test_fa_with_complexity(
        "src/x.rs",
        6,
        ComplexityMetrics {
            magic_numbers: vec![MagicNumberOccurrence {
                line: 6,
                value: "42".into(),
            }],
            ..Default::default()
        },
    );
    assert_eq!(
        complexity_orphans_for(fa),
        1,
        "magic number on a test fn is no target → marker is orphan"
    );
}

#[test]
fn error_handling_skipped_for_test_fn_leaves_marker_orphan() {
    // A panic in a *test* function is not an error-handling finding, so a
    // `qual:allow(complexity)` marker over it has no target → orphan. The
    // `!detect_error_handling || f.is_test` short-circuit flipped to `&&` would
    // count the panic and wrongly match the marker.
    let fa = make_test_fa_with_complexity(
        "src/x.rs",
        6,
        ComplexityMetrics {
            panic_count: 1,
            ..Default::default()
        },
    );
    assert_eq!(
        complexity_orphans_for(fa),
        1,
        "panic in a test fn is no error-handling target → marker is orphan"
    );
}
