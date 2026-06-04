//! Rendering tests for the text summary section. `format_summary_section`
//! orchestrates the header, per-dimension scores (with inline finding
//! locations), suppression lines, and the pass/fail footer; asserting on the
//! rendered string pins the arithmetic, thresholds, and category mapping that
//! coarse "contains 'quality'" smoke tests miss.
use crate::adapters::report::text::summary::format_summary_section;
use crate::report::findings_list::FindingEntry;
use crate::report::Summary;

fn entry(file: &str, line: usize, category: &'static str, function_name: &str) -> FindingEntry {
    FindingEntry {
        file: file.into(),
        line,
        category,
        detail: String::new(),
        function_name: function_name.into(),
    }
}

/// A summary with one finding in every dimension + a violation (7 findings,
/// ≤ 10 so inline locations show), exact dimension_scores, and suppressions —
/// rendered with one matching `FindingEntry` per dimension category.
fn rich_render() -> String {
    let mut summary = Summary {
        total: 100,
        integrations: 40,
        operations: 50,
        trivial: 9,
        violations: 1,
        complexity_warnings: 1,
        duplicate_groups: 1,
        srp_struct_warnings: 1,
        coupling_warnings: 1,
        tq_no_assertion_warnings: 1,
        architecture_warnings: 1,
        suppressed: 2,
        all_suppressions: 3,
        suppression_ratio_exceeded: true,
        ..Default::default()
    };
    summary.dimension_scores = [0.50, 0.60, 0.70, 0.80, 0.90, 0.95, 0.99];
    let findings = vec![
        entry("cx.rs", 11, "COGNITIVE", "cx_fn"),
        entry("dry.rs", 22, "DUPLICATE", "dry_fn"),
        entry("srp.rs", 33, "SRP_STRUCT", "srp_fn"),
        entry("cp.rs", 44, "COUPLING", "cp_fn"),
        entry("tq.rs", 55, "TQ_NO_ASSERT", "tq_fn"),
        entry("arch.rs", 66, "ARCHITECTURE", "arch_fn"),
    ];
    format_summary_section(&summary, &findings)
}

#[test]
fn format_summary_section_header_shows_counts_score_and_iosp_detail() {
    let out = rich_render();
    assert!(out.contains("Functions: 100"), "{out}");
    assert!(out.contains("7 findings"), "plural finding count: {out}");
    assert!(
        out.contains("40I, 50O, 9T, 1 violation)"),
        "IOSP detail with singular violation (pins `violations == 1`): {out}"
    );
    assert!(
        !out.contains("All quality checks passed"),
        "footer must not show with findings: {out}"
    );
}

#[test]
fn format_summary_section_shows_each_dimension_score_and_label() {
    let out = rich_render();
    // Exact percentages pin pct * MULTIPLIER.
    assert!(out.contains("50.0%"), "IOSP 0.50 → 50.0%: {out}");
    assert!(out.contains("60.0%"), "Complexity 0.60 → 60.0%: {out}");
    assert!(out.contains("99.0%"), "Architecture 0.99 → 99.0%: {out}");
    // Every label present pins build_dimensions != empty/default.
    for label in [
        "Complexity:",
        "DRY:",
        "SRP:",
        "Coupling:",
        "Test Quality:",
        "Architecture:",
    ] {
        assert!(
            out.contains(label),
            "dimension label {label} missing: {out}"
        );
    }
}

#[test]
fn format_summary_section_maps_each_category_to_its_dimension_locations() {
    // One inline location per dimension proves the dimension_categories arms map
    // each category to the right dimension.
    let out = rich_render();
    for loc in [
        "cx.rs:11",
        "dry.rs:22",
        "srp.rs:33",
        "cp.rs:44",
        "tq.rs:55",
        "arch.rs:66",
    ] {
        assert!(out.contains(loc), "inline location {loc} missing: {out}");
    }
}

#[test]
fn format_summary_section_renders_suppression_lines() {
    let out = rich_render();
    assert!(out.contains("Suppressed:   2"), "suppressed count: {out}");
    assert!(out.contains("All allows:   3"), "all-allows count: {out}");
    assert!(out.contains("ratio exceeds"), "ratio warning: {out}");
}

#[test]
fn format_summary_section_clean_shows_pass_footer_and_no_finding_count() {
    // Zero findings: header omits the finding count, and the pass footer shows.
    let mut summary = Summary {
        total: 10,
        integrations: 5,
        operations: 5,
        ..Default::default()
    };
    summary.dimension_scores = [1.0; 7];
    let out = format_summary_section(&summary, &[]);
    assert!(
        out.contains("All quality checks passed"),
        "clean run shows the pass footer: {out}"
    );
    assert!(
        !out.contains("finding"),
        "clean header omits the finding count: {out}"
    );
    // No violations → IOSP detail without a violation clause.
    assert!(out.contains("5I, 5O, 0T"), "{out}");
    assert!(!out.contains("violation"), "no violation clause: {out}");
}

#[test]
fn format_summary_section_detail_lists_only_nonzero_counts() {
    // Within a dimension only non-zero finding counts are listed (the `*c > 0`
    // filter). complexity_warnings = 2, the rest of Complexity's fields 0.
    let mut summary = Summary {
        total: 100,
        complexity_warnings: 2,
        ..Default::default()
    };
    summary.dimension_scores = [1.0, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0];
    let out = format_summary_section(&summary, &[]);
    assert!(out.contains("2 complexity"), "non-zero count listed: {out}");
    assert!(
        !out.contains("magic numbers"),
        "zero-count categories are not listed: {out}"
    );
}

#[test]
fn format_summary_section_hides_inline_locations_above_threshold() {
    // With more than INLINE_LOCATION_THRESHOLD (10) findings, inline `→` location
    // lines are suppressed even though findings exist. Pins the `<=` threshold.
    let mut summary = Summary {
        total: 100,
        complexity_warnings: 11,
        ..Default::default()
    };
    summary.dimension_scores = [1.0, 0.4, 1.0, 1.0, 1.0, 1.0, 1.0];
    let findings = vec![entry("cx.rs", 7, "COGNITIVE", "cx_fn")];
    let out = format_summary_section(&summary, &findings);
    assert!(out.contains("11 complexity"), "count still shown: {out}");
    assert!(
        !out.contains("cx.rs:7"),
        "inline locations hidden above the threshold: {out}"
    );
}

fn clean(total: usize) -> Summary {
    let mut s = Summary {
        total,
        ..Default::default()
    };
    s.dimension_scores = [1.0; 7];
    s
}

#[test]
fn summary_suppression_lines_only_for_nonzero_counts() {
    // suppressed = 0 → no "Suppressed:" line; all_suppressions = 3 → shown.
    // Pins the `> 0` guards against `>=` (which would print a "… 0" line).
    let mut s = clean(10);
    s.suppressed = 0;
    s.all_suppressions = 3;
    let out = format_summary_section(&s, &[]);
    assert!(
        !out.contains("Suppressed:"),
        "0 suppressed → no line: {out}"
    );
    assert!(out.contains("All allows:   3"), "all-allows shown: {out}");

    let mut s2 = clean(10);
    s2.suppressed = 0;
    s2.all_suppressions = 0;
    let out2 = format_summary_section(&s2, &[]);
    assert!(
        !out2.contains("All allows:"),
        "0 all-allows → no line: {out2}"
    );
    assert!(!out2.contains("Suppressed:"), "{out2}");
}

#[test]
fn summary_inline_location_skips_empty_file_findings() {
    // An inline location is shown only for a finding with a non-empty file AND a
    // matching dimension category. A complexity finding with an empty file must
    // NOT produce a `→ :line` line — pins `dim_cats.contains && !file.is_empty()`.
    let mut s = clean(100);
    s.complexity_warnings = 1;
    s.dimension_scores = [1.0, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0];
    let findings = vec![entry("", 7, "COGNITIVE", "ghost_fn")];
    let out = format_summary_section(&s, &findings);
    assert!(out.contains("1 complexity"), "count shown: {out}");
    assert!(
        !out.contains("ghost_fn"),
        "empty-file finding has no inline location: {out}"
    );
}

/// True if a blank line immediately precedes the first line containing `needle`.
fn blank_line_before(out: &str, needle: &str) -> bool {
    let lines: Vec<&str> = out.lines().collect();
    lines
        .iter()
        .position(|l| l.contains(needle))
        .is_some_and(|i| i > 0 && lines[i - 1].trim().is_empty())
}

#[test]
fn summary_suppression_section_preceded_by_blank_line() {
    // The suppression section is separated from the dimension scores by a blank
    // line (the `suppressed > 0 || all_suppressions > 0` guard). Pins the `||`
    // and the `> 0` comparisons: with exactly one count non-zero, dropping the
    // guard (`&&`, or `>`→`<`/`==`) removes the separator.
    let mut s = clean(10);
    s.suppressed = 2;
    s.all_suppressions = 0;
    assert!(
        blank_line_before(&format_summary_section(&s, &[]), "Suppressed:"),
        "blank line before the suppressed-count line"
    );
    let mut s2 = clean(10);
    s2.suppressed = 0;
    s2.all_suppressions = 3;
    assert!(
        blank_line_before(&format_summary_section(&s2, &[]), "All allows:"),
        "blank line before the all-allows line"
    );
}
