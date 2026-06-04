//! Tests for the `--explain allow` suppression guide. The guide is built from
//! the shared target vocabulary, so these pin the grammar + that each kind of
//! target surfaces for the right dimension.
use super::suppression_guide;

#[test]
fn suppression_guide_covers_grammar_targets_and_rationale() {
    let g = suppression_guide();
    // Grammar: metric value mandatory, reason mandatory.
    assert!(
        g.contains("qual:allow(dim, target=N)"),
        "metric grammar line"
    );
    assert!(g.contains("value REQUIRED"), "value-required note");
    assert!(g.contains("needs a reason"), "reason-required note");
    // Targets surface under the right dimension/kind.
    assert!(g.contains("file_length"), "srp metric target");
    assert!(g.contains("god_struct"), "srp boolean target");
    assert!(
        g.contains("max_instability"),
        "coupling float metric target"
    );
    assert!(g.contains("untested"), "test_quality target");
    // iosp is single-kind.
    assert!(g.contains("bare allow(iosp)"), "iosp single-kind note");
    // The pin rationale (10% + last resort).
    assert!(g.contains("10%"), "10% headroom rationale");
    assert!(g.contains("last resort"), "last-resort framing");
}
