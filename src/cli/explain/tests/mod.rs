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

#[test]
fn rule_card_text_renders_full_card_for_known_id() {
    let text = super::rule_card_text("bp-009").expect("BP-009 must render (case-insensitive)");
    assert!(text.contains("BP-009"), "card must show the canonical id");
    assert!(
        text.contains("Struct update boilerplate"),
        "card must show the title: {text}"
    );
    for section in ["Detects:", "Why:", "Fix:", "Suppress:", "Config:"] {
        assert!(text.contains(section), "card must have section {section}");
    }
    assert!(
        text.contains("qual:allow(dry, boilerplate)"),
        "card must cite the copyable suppression marker"
    );
    assert!(
        text.contains("[boilerplate]"),
        "card must point at the config section"
    );
}

#[test]
fn rule_card_text_is_none_for_unknown_id() {
    assert!(super::rule_card_text("BP-999").is_none());
    assert!(super::rule_card_text("src/lib.rs").is_none());
}

#[test]
fn fallback_text_lists_all_three_modes_with_examples() {
    // The no-match case must never dead-end: it names the argument and shows
    // every mode with a copyable example, including where rule ids come from.
    let g = super::explain_fallback_text("BP-999");
    assert!(g.contains("BP-999"), "must name the unrecognized argument");
    assert!(g.contains("--explain allow"), "must show the allow mode");
    assert!(
        g.contains("--explain BP-009"),
        "must show a rule-id example: {g}"
    );
    assert!(
        g.contains(".rs"),
        "must show the file-diagnostics mode: {g}"
    );
    assert!(
        g.contains("findings output"),
        "must say where rule ids appear: {g}"
    );
}

#[test]
fn fallback_text_hints_when_arg_is_a_suppression_target() {
    // `--explain boilerplate` was a real flailing agent's guess: the word is
    // a suppression target, not a rule id — say so and point at both paths.
    let g = super::explain_fallback_text("boilerplate");
    assert!(
        g.contains("suppression target"),
        "must identify the arg as a target name: {g}"
    );
    assert!(g.contains("dry"), "must name the owning dimension: {g}");
    assert!(
        g.contains("--explain allow"),
        "must point at the guide: {g}"
    );
}
