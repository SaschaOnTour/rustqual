//! Orphan detection at the exact complexity thresholds. A `qual:allow(complexity)`
//! marker over a function whose metric is *exactly* at the cap suppresses
//! nothing (the warning fires only when *over* the cap) → it is an orphan.
//! A `>`→`>=` mutation in the `exceeds_*` predicates would make `would_trigger`
//! true and wrongly clear the orphan.
use super::*;
use crate::adapters::analyzers::iosp::ComplexityMetrics;

#[test]
fn complexity_marker_at_exact_threshold_is_orphan() {
    // Defaults: cognitive 15, cyclomatic 10, nesting 4, length 60.
    let cases = [
        (
            "cognitive",
            ComplexityMetrics {
                cognitive_complexity: 15,
                ..Default::default()
            },
        ),
        (
            "cyclomatic",
            ComplexityMetrics {
                cyclomatic_complexity: 10,
                ..Default::default()
            },
        ),
        (
            "nesting",
            ComplexityMetrics {
                max_nesting: 4,
                ..Default::default()
            },
        ),
        (
            "length",
            ComplexityMetrics {
                function_lines: 60,
                ..Default::default()
            },
        ),
    ];
    for (label, m) in cases {
        let orphans = complexity_sup_orphans(5, 6, m);
        assert!(
            !orphans.is_empty(),
            "case {label}: a marker over an exactly-at-threshold fn suppresses nothing → orphan"
        );
    }
}

#[test]
fn error_handling_marker_is_not_orphan() {
    // A non-test fn that triggers error-handling makes the marker valid (not
    // orphan). Two single-count fixtures pin every `+` in the
    // `unwrap + panic + todo + expect.min(threshold)` sum: a lone `panic`
    // exercises the left `+`s, and a lone `expect` (threshold 1 under the
    // default `allow_expect=false`) exercises the trailing `+ expect.min`.
    // Any `+`→`*` collapses the live term to 0 (→ orphan); any `+`→`-`
    // underflows (→ panic). Either way the empty assertion catches it.
    let cases = [
        (
            "panic",
            ComplexityMetrics {
                panic_count: 1,
                ..Default::default()
            },
        ),
        (
            "expect",
            ComplexityMetrics {
                expect_count: 1,
                ..Default::default()
            },
        ),
    ];
    for (label, m) in cases {
        assert!(
            complexity_sup_orphans(5, 6, m).is_empty(),
            "case {label}: a non-test fn error-handling count is a target the marker suppresses"
        );
    }
}
