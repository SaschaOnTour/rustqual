//! Per-kind (category, detail) mapping tests for the findings-list reporter,
//! exercised through the public `collect_all_findings`. Each finding kind must
//! produce its exact category + detail string — pinning the match arms and
//! `-> ""`/"xyzzy" body replacements in `categories.rs`.
use crate::domain::findings::{
    ComplexityFinding, ComplexityFindingKind, DryFinding, DryFindingDetails, DryFindingKind,
    SrpFinding, SrpFindingDetails, SrpFindingKind,
};
use crate::domain::{AnalysisData, AnalysisFindings, Dimension, Finding, Severity};
use crate::report::findings_list::{collect_all_findings, FindingEntry};
use crate::report::{AnalysisResult, Summary};

fn common(dim: Dimension) -> Finding {
    Finding {
        file: "lib.rs".into(),
        line: 3,
        column: 0,
        dimension: dim,
        rule_id: "r".into(),
        message: "the magic number here".into(),
        severity: Severity::Medium,
        suppressed: false,
    }
}

fn collect(findings: AnalysisFindings) -> Vec<FindingEntry> {
    collect_all_findings(&AnalysisResult {
        results: vec![],
        summary: Summary::default(),
        findings,
        data: AnalysisData::default(),
    })
}

fn one_dry(kind: DryFindingKind, details: DryFindingDetails) -> FindingEntry {
    let f = DryFinding {
        common: common(Dimension::Dry),
        kind,
        details,
    };
    collect(AnalysisFindings {
        dry: vec![f],
        ..Default::default()
    })
    .into_iter()
    .next()
    .expect("one dry entry")
}

fn one_srp(kind: SrpFindingKind, details: SrpFindingDetails) -> FindingEntry {
    let f = SrpFinding {
        common: common(Dimension::Srp),
        kind,
        details,
    };
    collect(AnalysisFindings {
        srp: vec![f],
        ..Default::default()
    })
    .into_iter()
    .next()
    .expect("one srp entry")
}

fn one_complexity(kind: ComplexityFindingKind, metric_value: usize) -> FindingEntry {
    let f = ComplexityFinding {
        common: common(Dimension::Complexity),
        kind,
        metric_value,
        threshold: 0,
        hotspot: None,
    };
    collect(AnalysisFindings {
        complexity: vec![f],
        ..Default::default()
    })
    .into_iter()
    .next()
    .expect("one complexity entry")
}

type DryCase = (
    DryFindingKind,
    DryFindingDetails,
    &'static str,
    &'static str,
);

fn dry_cases() -> Vec<DryCase> {
    vec![
        (
            DryFindingKind::DuplicateSimilar,
            DryFindingDetails::Duplicate {
                participants: vec![],
                similarity: Some(0.9),
            },
            "DUPLICATE",
            "similar",
        ),
        (
            DryFindingKind::Fragment,
            DryFindingDetails::Fragment {
                participants: vec![],
                statement_count: 3,
            },
            "FRAGMENT",
            "3 stmts",
        ),
        (
            DryFindingKind::DeadCodeUncalled,
            DryFindingDetails::DeadCode {
                qualified_name: "ghost".into(),
                suggestion: None,
            },
            "DEAD_CODE",
            "ghost",
        ),
        (
            DryFindingKind::DeadCodeTestOnly,
            DryFindingDetails::DeadCode {
                qualified_name: "ghost".into(),
                suggestion: None,
            },
            "DEAD_CODE",
            "testonly ghost",
        ),
        (
            DryFindingKind::Wildcard,
            DryFindingDetails::Wildcard {
                module_path: "m::*".into(),
            },
            "WILDCARD",
            "m::*",
        ),
        (
            DryFindingKind::Boilerplate,
            DryFindingDetails::Boilerplate {
                pattern_id: "BP-007".into(),
                struct_name: None,
                suggestion: "x".into(),
            },
            "BOILERPLATE",
            "BP-007",
        ),
        (
            DryFindingKind::RepeatedMatch,
            DryFindingDetails::RepeatedMatch {
                enum_name: "Color".into(),
                participants: vec![],
            },
            "REPEATED_MATCH",
            "Color",
        ),
    ]
}

#[test]
fn dry_category_and_detail_per_kind() {
    for (kind, details, cat, detail) in dry_cases() {
        let e = one_dry(kind, details);
        assert_eq!(e.category, cat, "{kind:?} category");
        assert_eq!(e.detail, detail, "{kind:?} detail");
    }
}

#[test]
fn srp_category_and_detail_per_kind() {
    let struct_e = one_srp(
        SrpFindingKind::StructCohesion,
        SrpFindingDetails::StructCohesion {
            struct_name: "Big".into(),
            lcom4: 4,
            field_count: 9,
            method_count: 7,
            fan_out: 2,
            composite_score: 0.0,
            clusters: vec![],
        },
    );
    assert_eq!(
        (struct_e.category, struct_e.detail.as_str()),
        ("SRP_STRUCT", "Big: LCOM4=4")
    );
    let module_e = one_srp(
        SrpFindingKind::ModuleLength,
        SrpFindingDetails::ModuleLength {
            module: "m".into(),
            production_lines: 900,
            independent_clusters: 0,
            cluster_names: vec![],
            length_score: 0.0,
        },
    );
    assert_eq!(
        (module_e.category, module_e.detail.as_str()),
        ("SRP_MODULE", "900 lines")
    );
    let param_e = one_srp(
        SrpFindingKind::ParameterCount,
        SrpFindingDetails::ParameterCount {
            function_name: "f".into(),
            parameter_count: 7,
        },
    );
    assert_eq!(
        (param_e.category, param_e.detail.as_str()),
        ("SRP_PARAMS", "7 params")
    );
    let struct_code = one_srp(
        SrpFindingKind::Structural,
        SrpFindingDetails::Structural {
            item_name: "I".into(),
            code: "SLM".into(),
            detail: "d".into(),
        },
    );
    assert_eq!(
        (struct_code.category, struct_code.detail.as_str()),
        ("SRP_STRUCTURAL", "SLM")
    );
}

#[test]
fn complexity_detail_per_kind() {
    use ComplexityFindingKind::*;
    assert_eq!(one_complexity(Cognitive, 10).detail, "complexity 10");
    assert_eq!(one_complexity(NestingDepth, 5).detail, "depth 5");
    assert_eq!(one_complexity(FunctionLength, 80).detail, "80 lines");
    assert_eq!(one_complexity(Unsafe, 2).detail, "2 blocks");
    assert_eq!(one_complexity(ErrorHandling, 0).detail, "unwrap/panic/todo");
}

#[test]
fn srp_build_drops_suppressed() {
    // A suppressed SRP finding produces no entry — pins build_srp's `!suppressed`
    // filter and its `-> vec![]` replacement (the unsuppressed one still emits).
    let mut suppressed = SrpFinding {
        common: common(Dimension::Srp),
        kind: SrpFindingKind::ParameterCount,
        details: SrpFindingDetails::ParameterCount {
            function_name: "f".into(),
            parameter_count: 7,
        },
    };
    suppressed.common.suppressed = true;
    let entries = collect(AnalysisFindings {
        srp: vec![suppressed],
        ..Default::default()
    });
    assert!(
        !entries.iter().any(|e| e.category.starts_with("SRP")),
        "suppressed SRP finding excluded: {entries:?}"
    );
}

fn frec(
    file: &str,
    line: usize,
    qualified_name: &str,
) -> crate::domain::analysis_data::FunctionRecord {
    crate::domain::analysis_data::FunctionRecord {
        name: qualified_name.into(),
        file: file.into(),
        line,
        qualified_name: qualified_name.into(),
        parent_type: None,
        classification: crate::domain::analysis_data::FunctionClassification::Operation,
        severity: None,
        complexity: None,
        parameter_count: 0,
        own_calls: vec![],
        is_trait_impl: false,
        is_test: false,
        effort_score: None,
        suppressed: false,
        complexity_suppressed: false,
    }
}

fn cog_entry_with_functions(
    functions: Vec<crate::domain::analysis_data::FunctionRecord>,
) -> FindingEntry {
    let f = ComplexityFinding {
        common: common(Dimension::Complexity),
        kind: ComplexityFindingKind::Cognitive,
        metric_value: 10,
        threshold: 0,
        hotspot: None,
    };
    collect_all_findings(&AnalysisResult {
        results: vec![],
        summary: Summary::default(),
        findings: AnalysisFindings {
            complexity: vec![f],
            ..Default::default()
        },
        data: AnalysisData {
            functions,
            ..Default::default()
        },
    })
    .into_iter()
    .next()
    .expect("one entry")
}

#[test]
fn function_name_at_requires_exact_file_and_line_match() {
    // The finding sits at lib.rs:3. An exact-match record supplies its name;
    // records that differ in file OR line must not match (pins the
    // `fr.file == file && fr.line == line` lookup against `&&`→`||` / `==`→`!=`).
    let exact = cog_entry_with_functions(vec![frec("lib.rs", 3, "right_fn")]);
    assert_eq!(exact.function_name, "right_fn", "exact match supplies name");

    let mismatched = cog_entry_with_functions(vec![
        frec("lib.rs", 99, "wrong_line"),
        frec("other.rs", 3, "wrong_file"),
    ]);
    assert_eq!(
        mismatched.function_name, "",
        "no record matches both file and line → empty name"
    );
}
