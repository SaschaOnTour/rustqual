use super::*;

// ── build_ai_value: shape contract ─────────────────────────────────

#[test]
fn build_ai_value_zero_findings_no_findings_by_file() {
    let analysis = empty_analysis();
    let config = Config::default();
    let value = build_ai_value(&analysis, &config);
    assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(value["findings"], 0);
    assert!(
        value.get("findings_by_file").is_none(),
        "no findings_by_file when 0 findings"
    );
}

#[test]
fn build_ai_value_includes_architecture_finding() {
    let mut analysis = empty_analysis();
    analysis.findings.architecture = vec![ArchitectureFinding {
        common: Finding {
            file: "src/cli/handlers.rs".into(),
            line: 17,
            column: 0,
            dimension: crate::findings::Dimension::Architecture,
            rule_id: "architecture/call_parity/no_delegation".into(),
            message: "cli pub fn delegates to no application function".into(),
            severity: crate::domain::Severity::Medium,
            suppressed: false,
        },
    }];
    let config = Config::default();
    let value = build_ai_value(&analysis, &config);

    assert_eq!(value["findings"], 1);
    let by_file = value["findings_by_file"]
        .as_object()
        .expect("findings_by_file present when findings > 0");
    let entries = by_file["src/cli/handlers.rs"]
        .as_array()
        .expect("entries for the architecture finding's file");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["category"], "architecture");
    assert_eq!(entries[0]["line"], 17);
}

#[test]
fn build_ai_value_groups_entries_by_file() {
    let mut analysis = empty_analysis();
    analysis.findings.architecture = vec![
        ArchitectureFinding {
            common: arch_common("src/a.rs", 10, crate::domain::Severity::Medium),
        },
        ArchitectureFinding {
            common: arch_common("src/a.rs", 20, crate::domain::Severity::Medium),
        },
        ArchitectureFinding {
            common: arch_common("src/b.rs", 5, crate::domain::Severity::Medium),
        },
    ];
    let config = Config::default();
    let value = build_ai_value(&analysis, &config);
    assert_eq!(value["findings"], 3);
    let by_file = value["findings_by_file"].as_object().expect("by_file");
    assert_eq!(by_file.len(), 2);
    assert!(by_file.contains_key("src/a.rs"));
    assert!(by_file.contains_key("src/b.rs"));
    assert_eq!(by_file["src/a.rs"].as_array().unwrap().len(), 2);
    assert_eq!(by_file["src/b.rs"].as_array().unwrap().len(), 1);
}

#[test]
fn build_ai_value_skips_suppressed() {
    let mut analysis = empty_analysis();
    let mut common = arch_common("src/foo.rs", 5, crate::domain::Severity::Medium);
    common.suppressed = true;
    analysis.findings.architecture = vec![ArchitectureFinding { common }];
    let config = Config::default();
    let value = build_ai_value(&analysis, &config);
    assert_eq!(value["findings"], 0);
    assert!(value.get("findings_by_file").is_none());
}

#[test]
fn every_entry_has_the_same_keys() {
    // The AI format renders each file's entries as a TOON table, and a table
    // needs one shape: `toon-encode` falls back to a per-entry list as soon as
    // two objects differ in their keys. That fallback is still correct output,
    // but a consumer counting rows in the tabular form silently misses the rest
    // — a downstream agent lost 50 of 138 findings that way. Orphan entries
    // carry a `kind`; everything else must carry it too, empty.
    let mut analysis = empty_analysis();
    analysis.findings.architecture = vec![ArchitectureFinding {
        common: Finding {
            file: "src/a.rs".into(),
            line: 17,
            column: 0,
            dimension: crate::findings::Dimension::Architecture,
            rule_id: "architecture/call_parity/no_delegation".into(),
            message: "m".into(),
            severity: crate::domain::Severity::Medium,
            suppressed: false,
        },
    }];
    analysis.findings.orphan_suppressions = vec![crate::domain::findings::OrphanSuppression {
        marker: crate::domain::findings::MarkerKind::Api,
        file: "src/a.rs".into(),
        line: 3,
        dimensions: Vec::new(),
        target: None,
        reason: Some("r".into()),
        kind: crate::domain::findings::OrphanKind::Stale,
    }];
    let value = build_ai_value(&analysis, &Config::default());

    let entries = value["findings_by_file"]["src/a.rs"]
        .as_array()
        .expect("both entries land in one file bucket");
    assert_eq!(entries.len(), 2);
    let shapes: std::collections::HashSet<Vec<String>> = entries
        .iter()
        .map(|e| {
            let mut keys: Vec<String> = e
                .as_object()
                .expect("object entry")
                .keys()
                .cloned()
                .collect();
            keys.sort();
            keys
        })
        .collect();
    assert_eq!(
        shapes.len(),
        1,
        "one row shape, or the table falls back to a list: {shapes:?}"
    );
}
