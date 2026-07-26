use super::*;

// ── Orphan-suppression SARIF coverage ─────────────────────────

#[test]
fn sarif_reporter_emits_orphan_results_via_snapshot_view() {
    use crate::domain::findings::OrphanSuppression;
    let mut analysis = make_analysis(vec![]);
    // Trait-driven path — populate `findings.orphan_suppressions`
    // (NOT the legacy `analysis.orphan_suppressions` field).
    analysis.findings.orphan_suppressions = vec![OrphanSuppression {
        marker: crate::domain::findings::MarkerKind::Allow,
        file: "src/foo.rs".into(),
        line: 42,
        dimensions: vec![crate::findings::Dimension::Srp],
        reason: Some("legacy marker".into()),
        target: None,
        kind: crate::domain::findings::OrphanKind::Stale,
    }];
    let orphan = sarif_result_by_rule(&analysis, "ORPHAN-001");
    assert_eq!(orphan["level"], "warning");
    assert_eq!(
        orphan["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
        "src/foo.rs"
    );
    assert_eq!(
        orphan["locations"][0]["physicalLocation"]["region"]["startLine"],
        42
    );
    let msg = orphan["message"]["text"].as_str().expect("message text");
    assert!(
        msg.contains("srp"),
        "message should name suppressed dim: {msg}"
    );
    assert!(
        msg.contains("legacy marker"),
        "message should carry reason: {msg}"
    );
}

#[test]
fn sarif_rules_include_orphan_suppression() {
    let analysis = make_analysis(vec![]);
    let value = build_sarif_value(&analysis);
    let rules = value["runs"][0]["tool"]["driver"]["rules"]
        .as_array()
        .expect("rules array");
    let orphan_rule = rules
        .iter()
        .find(|r| r["id"] == "ORPHAN-001")
        .expect("ORPHAN-001 rule present in tool.driver.rules");
    let desc = orphan_rule["shortDescription"]["text"]
        .as_str()
        .expect("shortDescription text");
    assert!(
        desc.to_lowercase().contains("orphan") || desc.to_lowercase().contains("stale"),
        "rule description should name the orphan concept: {desc}"
    );
}

// ── Architecture findings SARIF coverage (v1.2.1) ─────────────

fn make_arch_finding(rule_id: &str, severity: crate::domain::Severity) -> crate::domain::Finding {
    crate::domain::Finding {
        file: "src/cli/handlers.rs".to_string(),
        line: 17,
        column: 0,
        dimension: crate::findings::Dimension::Architecture,
        rule_id: rule_id.to_string(),
        severity,
        message: format!("test message for {rule_id}"),
        suppressed: false,
    }
}

#[test]
fn sarif_emits_architecture_call_parity_finding() {
    let mut analysis = make_analysis(vec![]);
    analysis.findings.architecture = vec![crate::domain::findings::ArchitectureFinding {
        common: make_arch_finding(
            "architecture/call_parity/no_delegation",
            crate::domain::Severity::Medium,
        ),
    }];
    let hit = sarif_result_by_rule(&analysis, "architecture/call_parity/no_delegation");
    assert_eq!(hit["level"], "warning");
    assert_eq!(
        hit["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
        "src/cli/handlers.rs"
    );
    assert_eq!(
        hit["locations"][0]["physicalLocation"]["region"]["startLine"],
        17
    );
}

#[test]
fn sarif_maps_architecture_severities() {
    let mut analysis = make_analysis(vec![]);
    analysis.findings.architecture = vec![
        crate::domain::findings::ArchitectureFinding {
            common: make_arch_finding(
                "architecture/call_parity/multi_touchpoint",
                crate::domain::Severity::Low,
            ),
        },
        crate::domain::findings::ArchitectureFinding {
            common: make_arch_finding(
                "architecture/call_parity/missing_adapter",
                crate::domain::Severity::Medium,
            ),
        },
        crate::domain::findings::ArchitectureFinding {
            common: make_arch_finding(
                "architecture/trait_contract/object_safety",
                crate::domain::Severity::High,
            ),
        },
    ];
    let value = build_sarif_value(&analysis);
    let results = value["runs"][0]["results"]
        .as_array()
        .expect("results array");
    let level_for = |rid: &str| -> &str {
        results
            .iter()
            .find(|r| r["ruleId"] == rid)
            .unwrap_or_else(|| panic!("missing {rid}"))["level"]
            .as_str()
            .expect("level string")
    };
    assert_eq!(
        level_for("architecture/call_parity/multi_touchpoint"),
        "note"
    );
    assert_eq!(
        level_for("architecture/call_parity/missing_adapter"),
        "warning"
    );
    assert_eq!(
        level_for("architecture/trait_contract/object_safety"),
        "error"
    );
}

#[test]
fn sarif_skips_suppressed_architecture_findings() {
    let mut analysis = make_analysis(vec![]);
    let mut suppressed = make_arch_finding(
        "architecture/call_parity/no_delegation",
        crate::domain::Severity::Medium,
    );
    suppressed.suppressed = true;
    analysis.findings.architecture =
        vec![crate::domain::findings::ArchitectureFinding { common: suppressed }];
    let value = build_sarif_value(&analysis);
    let results = value["runs"][0]["results"]
        .as_array()
        .expect("results array");
    assert!(
        !results
            .iter()
            .any(|r| r["ruleId"] == "architecture/call_parity/no_delegation"),
        "suppressed architecture finding must not appear in SARIF"
    );
}
