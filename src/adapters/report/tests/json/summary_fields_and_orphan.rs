use super::*;

// ── JSON content tests (verifying fields are present) ──────

#[test]
fn test_json_summary_warning_count_fields_are_numbers() {
    let analysis = make_analysis(vec![make_result("f", Classification::Operation)]);
    let parsed = json_value(&analysis);
    for field in ["complexity_warnings", "magic_number_warnings"] {
        assert!(
            parsed["summary"][field].is_number(),
            "JSON summary must include numeric `{field}` field"
        );
    }
}

#[test]
fn test_json_summary_has_all_dimension_fields() {
    let analysis = make_analysis(vec![make_result("f", Classification::Operation)]);
    let parsed = json_value(&analysis);
    let s = &parsed["summary"];
    let expected_fields = [
        "total",
        "integrations",
        "operations",
        "violations",
        "trivial",
        "suppressed",
        "all_suppressions",
        "iosp_score",
        "quality_score",
        "complexity_warnings",
        "magic_number_warnings",
        "nesting_depth_warnings",
        "function_length_warnings",
        "unsafe_warnings",
        "error_handling_warnings",
        "coupling_warnings",
        "coupling_cycles",
        "duplicate_groups",
        "dead_code_warnings",
        "fragment_groups",
        "boilerplate_warnings",
        "srp_struct_warnings",
        "srp_module_warnings",
        "srp_param_warnings",
        "tq_no_assertion_warnings",
        "tq_no_sut_warnings",
        "tq_untested_warnings",
        "tq_uncovered_warnings",
        "tq_untested_logic_warnings",
        "suppression_ratio_exceeded",
    ];
    expected_fields.iter().for_each(|&field| {
        assert!(!s[field].is_null(), "JSON summary missing field: {field}");
    });
}

#[test]
fn test_json_complexity_has_extended_fields() {
    let mut func = make_result("f", Classification::Operation);
    func.complexity = Some(ComplexityMetrics {
        logic_count: 3,
        call_count: 1,
        max_nesting: 2,
        function_lines: 45,
        unsafe_blocks: 1,
        unwrap_count: 2,
        expect_count: 1,
        panic_count: 0,
        todo_count: 0,
        ..Default::default()
    });
    let analysis = make_analysis(vec![func]);
    let parsed = json_value(&analysis);
    let c = &parsed["functions"][0]["complexity"];
    assert_eq!(c["function_lines"].as_u64().unwrap(), 45);
    assert_eq!(c["unsafe_blocks"].as_u64().unwrap(), 1);
    assert_eq!(c["unwrap_count"].as_u64().unwrap(), 2);
    assert_eq!(c["expect_count"].as_u64().unwrap(), 1);
    assert_eq!(c["panic_count"].as_u64().unwrap(), 0);
    assert_eq!(c["todo_count"].as_u64().unwrap(), 0);
}

#[test]
fn json_reporter_includes_orphan_suppressions_via_snapshot_view() {
    // Populate `findings.orphan_suppressions` ONLY (not the legacy
    // `analysis.orphan_suppressions` field) and verify the JSON
    // output still includes the orphans — proving the JSON reporter
    // reads them from the trait-driven `Snapshot::orphans` view.
    use crate::domain::findings::OrphanSuppression;
    let mut analysis = make_analysis(vec![]);
    analysis.findings.orphan_suppressions = vec![OrphanSuppression {
        marker: crate::domain::findings::MarkerKind::Allow,
        file: "src/foo.rs".into(),
        line: 42,
        dimensions: vec![crate::findings::Dimension::Srp],
        reason: Some("legacy".into()),
        target: None,
        kind: crate::domain::findings::OrphanKind::Stale,
    }];
    let parsed = json_value(&analysis);
    let arr = parsed["orphan_suppressions"].as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["file"], "src/foo.rs");
    assert_eq!(arr[0]["line"], 42);
    assert_eq!(arr[0]["kind"], "stale");
    assert_eq!(arr[0]["dimensions"][0], "srp");
    assert_eq!(arr[0]["reason"], "legacy");
}

#[test]
fn json_orphan_projects_too_loose_kind_and_target() {
    // The structured format must carry the remedy (stale vs too_loose) and the
    // pinned target, so CI consumers don't have to parse the human message.
    use crate::domain::findings::{OrphanKind, OrphanSuppression};
    use crate::domain::SuppressionTarget;
    let mut analysis = make_analysis(vec![]);
    analysis.findings.orphan_suppressions = vec![OrphanSuppression {
        marker: crate::domain::findings::MarkerKind::Allow,
        file: "src/foo.rs".into(),
        line: 7,
        dimensions: vec![crate::findings::Dimension::Srp],
        target: Some(SuppressionTarget::Metric {
            name: "file_length".into(),
            pin: 400.0,
        }),
        reason: Some("tighten to ~305".into()),
        kind: OrphanKind::PinTooLoose,
    }];
    let o = &json_value(&analysis)["orphan_suppressions"][0];
    assert_eq!(o["kind"], "too_loose");
    assert_eq!(o["target"], "file_length=400");
}

#[test]
fn test_json_omits_empty_orphan_suppressions() {
    // When the list is empty (clean codebase), the field is elided
    // to keep JSON compact — matches the policy for other optional
    // arrays (duplicates, dead_code, etc.).
    let analysis = make_analysis(vec![]);
    let parsed = json_value(&analysis);
    assert!(
        parsed.get("orphan_suppressions").is_none(),
        "empty orphan list should be elided from JSON"
    );
}

#[test]
fn json_orphan_carries_the_marker_kind() {
    // A stale `qual:api` has no dimensions and no target, so without an
    // explicit marker field a JSON consumer cannot tell it from a blanket
    // `qual:allow` — and would report the wrong remedy.
    use crate::domain::findings::{MarkerKind, OrphanKind, OrphanSuppression};
    let mut analysis = make_analysis(vec![]);
    analysis.findings.orphan_suppressions = vec![OrphanSuppression {
        marker: MarkerKind::Api,
        file: "src/foo.rs".into(),
        line: 7,
        dimensions: vec![],
        reason: Some("production calls it".into()),
        target: None,
        kind: OrphanKind::Stale,
    }];
    let parsed = json_value(&analysis);
    let orphan = &parsed["orphan_suppressions"][0];
    assert_eq!(
        orphan["marker"], "api",
        "marker kind must round-trip: {orphan}"
    );
    assert_eq!(orphan["kind"], "stale");
}
