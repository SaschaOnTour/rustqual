//! AI detail-formatter edge cases: the partner-location filters in
//! `dry_category_detail` (self-link exclusion via `!(file == && line ==)`) and
//! the `independent_clusters > max` cluster clause in `srp_category_detail`.
use super::*;

fn dry_detail(f: DryFinding) -> String {
    let config = Config::default();
    let data = crate::domain::AnalysisData::default();
    let rows = make_reporter(&config, &data).build_dry(&[f]);
    let entries: Vec<Value> = rows.into_iter().map(format_dry_entry).collect();
    entries[0]["detail"].as_str().unwrap().to_string()
}

fn dry_common() -> Finding {
    Finding {
        file: "src/a.rs".into(),
        line: 10,
        column: 0,
        dimension: crate::findings::Dimension::Dry,
        rule_id: "r".into(),
        message: "x".into(),
        severity: crate::domain::Severity::Medium,
        suppressed: false,
    }
}

fn participant(file: &str, line: usize) -> DuplicateParticipant {
    DuplicateParticipant {
        function_name: "p".into(),
        file: file.into(),
        line,
    }
}

fn frag_participant(file: &str, line: usize) -> crate::domain::findings::FragmentParticipant {
    crate::domain::findings::FragmentParticipant {
        function_name: "p".into(),
        file: file.into(),
        line,
        end_line: line + 3,
    }
}

#[test]
fn duplicate_keeps_same_file_different_line_partner() {
    // A partner in the SAME file but a different line is a real partner; the
    // self-link (same file AND line) is excluded. Pins the `&&` (an `||` would
    // drop the same-file partner too).
    let detail = dry_detail(DryFinding {
        common: dry_common(),
        kind: DryFindingKind::DuplicateExact,
        details: DryFindingDetails::Duplicate {
            participants: vec![participant("src/a.rs", 10), participant("src/a.rs", 99)],
            similarity: None,
        },
    });
    assert!(
        detail.contains("src/a.rs:99"),
        "same-file partner kept: {detail}"
    );
}

#[test]
fn fragment_excludes_self_and_keeps_other_partners() {
    // Fragment partner filter: self (a.rs:10) excluded, same-file-other-line
    // (a.rs:99) and other-file (b.rs:20) kept. Pins the `!`, the two `==`, and
    // the `&&` of `!(p.file == f.file && p.line == f.line)`.
    let detail = dry_detail(DryFinding {
        common: dry_common(),
        kind: DryFindingKind::Fragment,
        details: DryFindingDetails::Fragment {
            participants: vec![
                frag_participant("src/a.rs", 10),
                frag_participant("src/a.rs", 99),
                frag_participant("src/b.rs", 20),
            ],
            statement_count: 4,
        },
    });
    assert!(
        detail.contains("src/a.rs:99"),
        "same-file partner kept: {detail}"
    );
    assert!(
        detail.contains("src/b.rs:20"),
        "other-file partner kept: {detail}"
    );
    assert!(
        !detail.contains("src/a.rs:10"),
        "self-link excluded: {detail}"
    );
}

#[test]
fn srp_module_cluster_clause_pins_greater_than_max() {
    let mut config = Config::default();
    config.srp.max_independent_clusters = 2;
    let data = crate::domain::AnalysisData::default();
    let module = |clusters: usize| SrpFinding {
        common: Finding {
            file: "m.rs".into(),
            line: 1,
            column: 0,
            dimension: crate::findings::Dimension::Srp,
            rule_id: "srp/module".into(),
            message: "x".into(),
            severity: crate::domain::Severity::Medium,
            suppressed: false,
        },
        kind: SrpFindingKind::ModuleLength,
        details: SrpFindingDetails::ModuleLength {
            module: "m".into(),
            production_lines: 900,
            independent_clusters: clusters,
            cluster_names: vec![],
            length_score: 0.0,
        },
    };
    let detail = |clusters| {
        let rows = make_reporter(&config, &data).build_srp(&[module(clusters)]);
        let entries: Vec<Value> = rows
            .into_iter()
            .map(|r| format_srp_entry(r, &config))
            .collect();
        entries[0]["detail"].as_str().unwrap().to_string()
    };
    assert!(
        detail(3).contains("independent clusters"),
        "3 > 2 → clause shown"
    );
    assert!(
        !detail(2).contains("independent clusters"),
        "2 not > 2 → clause hidden (pins `>` vs `>=`)"
    );
}

#[test]
fn iosp_function_name_requires_exact_line_match() {
    // A function record at lib.rs:40 must NOT resolve an IOSP finding at
    // lib.rs:99 (same file, different line). Pins `fr.file == file && fr.line ==
    // line` against `&&`→`||`, which would mis-resolve on file alone.
    let mut data = crate::domain::AnalysisData::default();
    data.functions.push(violation_record("src/lib.rs", 40));
    let f = iosp_finding("src/lib.rs", 99);
    let config = Config::default();
    let rows = make_reporter(&config, &data).build_iosp(&[f]);
    let entries: Vec<Value> = rows.into_iter().map(format_iosp_entry).collect();
    assert_ne!(
        entries[0]["fn"], "MyType::bad_fn",
        "line mismatch must not resolve the function name"
    );
}
