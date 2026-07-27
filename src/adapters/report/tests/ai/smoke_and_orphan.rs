use super::*;

// ── Smoke tests ────────────────────────────────────────────────────

#[test]
fn ai_toon_render_on_empty_analysis_emits_zero_findings() {
    let analysis = empty_analysis();
    let config = Config::default();
    let reporter = AiReporter {
        config: &config,
        coverage_mode: "call-graph-only",
        data: &analysis.data,
        format: AiOutputFormat::Toon,
    };
    let output = reporter.render(&analysis.findings, &analysis.data);
    assert!(
        output.contains("findings"),
        "TOON output must include the findings key; got {output}"
    );
    assert!(
        !output.contains("findings_by_file:"),
        "no findings_by_file when zero findings; got {output}"
    );
}

#[test]
fn ai_json_render_on_empty_analysis_emits_zero_findings() {
    let analysis = empty_analysis();
    let config = Config::default();
    let value = build_ai_value(&analysis, &config);
    assert_eq!(
        value["findings"], 0,
        "empty analysis must produce zero findings; got {value}"
    );
}

#[test]
fn ai_value_includes_complexity_finding_metric_and_location() {
    let mut analysis = empty_analysis();
    analysis.findings.complexity = vec![ComplexityFinding {
        common: Finding {
            file: "test.rs".into(),
            line: 4,
            column: 0,
            dimension: crate::findings::Dimension::Complexity,
            rule_id: "complexity/magic_number".into(),
            message: "magic number 42 in f".into(),
            severity: crate::domain::Severity::Medium,
            suppressed: false,
        },
        kind: ComplexityFindingKind::MagicNumber,
        metric_value: 1,
        threshold: 0,
        hotspot: None,
    }];
    let config = Config::default();
    let value = build_ai_value(&analysis, &config);
    assert_eq!(value["findings"], 1, "one complexity finding: got {value}");
    let by_file = value["findings_by_file"]
        .as_object()
        .expect("findings_by_file present");
    let entries = by_file
        .get("test.rs")
        .and_then(|v| v.as_array())
        .expect("entries for test.rs");
    let magic_entries: Vec<_> = entries
        .iter()
        .filter(|e| e["category"].as_str() == Some("magic_number"))
        .collect();
    assert_eq!(
        magic_entries.len(),
        1,
        "magic_number category preserved; got {value}"
    );
    assert_eq!(
        magic_entries[0]["line"], 4,
        "line number preserved; got {value}"
    );
}

#[test]
fn ai_reporter_includes_orphan_entries_via_snapshot_view() {
    use crate::domain::findings::OrphanSuppression;
    let mut analysis = empty_analysis();
    analysis.findings.orphan_suppressions = vec![OrphanSuppression {
        marker: crate::domain::findings::MarkerKind::Allow,
        file: "src/foo.rs".into(),
        line: 42,
        dimensions: vec![crate::findings::Dimension::Srp],
        reason: Some("legacy".into()),
        target: None,
        kind: crate::domain::findings::OrphanKind::Stale,
    }];
    let config = Config::default();
    let value = build_ai_value(&analysis, &config);
    assert_eq!(
        value["findings"], 1,
        "orphan must count as a finding, got value: {value}"
    );
    let by_file = value["findings_by_file"]
        .as_object()
        .expect("findings_by_file present");
    let entries = by_file
        .get("src/foo.rs")
        .and_then(|v| v.as_array())
        .expect("entries for src/foo.rs");
    let orphan = entries
        .iter()
        .find(|e| e["category"] == "orphan_suppression")
        .expect("orphan entry under src/foo.rs");
    assert_eq!(orphan["line"], 42);
    assert_eq!(orphan["kind"], "stale", "AI must project the orphan kind");
    assert!(
        orphan["detail"]
            .as_str()
            .unwrap_or_default()
            .starts_with("stale"),
        "AI detail must lead with the status word, got {orphan}"
    );
}
