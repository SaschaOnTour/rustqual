//! Orphan-suppression finding-builder fixtures for the warnings tests
//! (extracted so mod.rs stays ≤ the SRP file-length cap).

use super::*;

// ── detect_orphan_suppressions ─────────────────────────────────

pub(crate) fn empty_analysis() -> crate::report::AnalysisResult {
    crate::report::AnalysisResult {
        results: vec![],
        summary: Summary::default(),
        findings: crate::domain::AnalysisFindings::default(),
        data: crate::domain::AnalysisData::default(),
    }
}

pub(crate) fn make_finding(
    file: &str,
    line: usize,
    dimension: crate::findings::Dimension,
    rule_id: &str,
) -> crate::domain::Finding {
    crate::domain::Finding {
        file: file.into(),
        line,
        column: 0,
        dimension,
        rule_id: rule_id.into(),
        message: String::new(),
        severity: crate::domain::Severity::Medium,
        suppressed: false,
    }
}

pub(crate) fn make_srp_struct_finding(
    file: &str,
    line: usize,
) -> crate::domain::findings::SrpFinding {
    crate::domain::findings::SrpFinding {
        common: make_finding(
            file,
            line,
            crate::findings::Dimension::Srp,
            "srp/struct_cohesion",
        ),
        kind: crate::domain::findings::SrpFindingKind::StructCohesion,
        details: crate::domain::findings::SrpFindingDetails::StructCohesion {
            struct_name: "Foo".into(),
            lcom4: 3,
            field_count: 5,
            method_count: 5,
            fan_out: 2,
            composite_score: 0.0,
            clusters: vec![],
        },
    }
}

pub(crate) fn make_srp_module_finding(file: &str) -> crate::domain::findings::SrpFinding {
    crate::domain::findings::SrpFinding {
        common: make_finding(
            file,
            1,
            crate::findings::Dimension::Srp,
            "srp/module_length",
        ),
        kind: crate::domain::findings::SrpFindingKind::ModuleLength,
        details: crate::domain::findings::SrpFindingDetails::ModuleLength {
            module: file.into(),
            production_lines: 900,
            independent_clusters: 1,
            cluster_names: vec![],
            // A 900-line module has its length component active (score > 1.0);
            // the cohesion component is inactive (1 cluster <= max). Mirrors a
            // real "long but cohesive" finding so component-gated position
            // emission (file_length only) is exercised.
            length_score: 1.5,
        },
    }
}

pub(crate) fn make_srp_param_finding(
    file: &str,
    line: usize,
    suppressed: bool,
) -> crate::domain::findings::SrpFinding {
    let mut common = make_finding(
        file,
        line,
        crate::findings::Dimension::Srp,
        "srp/parameter_count",
    );
    common.suppressed = suppressed;
    crate::domain::findings::SrpFinding {
        common,
        kind: crate::domain::findings::SrpFindingKind::ParameterCount,
        details: crate::domain::findings::SrpFindingDetails::ParameterCount {
            function_name: "big_factory".into(),
            parameter_count: 7,
        },
    }
}

pub(crate) fn make_tq_finding(
    file: &str,
    line: usize,
    kind: crate::domain::findings::TqFindingKind,
) -> crate::domain::findings::TqFinding {
    crate::domain::findings::TqFinding {
        common: make_finding(
            file,
            line,
            crate::findings::Dimension::TestQuality,
            "tq/no_assertion",
        ),
        kind,
        function_name: "test_it".into(),
        uncovered_lines: None,
        coverage: crate::adapters::report::test_support::evidence_for(kind),
    }
}

pub(crate) fn make_dry_dead_code_finding(
    file: &str,
    line: usize,
) -> crate::domain::findings::DryFinding {
    crate::domain::findings::DryFinding {
        common: make_finding(
            file,
            line,
            crate::findings::Dimension::Dry,
            "dry/dead_code/uncalled",
        ),
        kind: crate::domain::findings::DryFindingKind::DeadCodeUncalled,
        details: crate::domain::findings::DryFindingDetails::DeadCode {
            qualified_name: "helper".into(),
            suggestion: None,
        },
    }
}

pub(crate) fn make_dry_wildcard_finding(
    file: &str,
    line: usize,
) -> crate::domain::findings::DryFinding {
    crate::domain::findings::DryFinding {
        common: make_finding(file, line, crate::findings::Dimension::Dry, "dry/wildcard"),
        kind: crate::domain::findings::DryFindingKind::Wildcard,
        details: crate::domain::findings::DryFindingDetails::Wildcard {
            module_path: "foo::*".into(),
        },
    }
}

/// A `suppressed` DUPLICATE finding — mirrors the projected state after a
/// `qual:allow(dry, duplicate)` marker clears its group (the group stays in
/// `findings.dry` with `suppressed = true`). The orphan detector must still
/// see its position so the marker that cleared it is not judged stale.
pub(crate) fn make_dry_duplicate_finding_suppressed(
    file: &str,
    line: usize,
) -> crate::domain::findings::DryFinding {
    let mut common = make_finding(
        file,
        line,
        crate::findings::Dimension::Dry,
        "dry/duplicate/exact",
    );
    common.suppressed = true;
    crate::domain::findings::DryFinding {
        common,
        kind: crate::domain::findings::DryFindingKind::DuplicateExact,
        details: crate::domain::findings::DryFindingDetails::Duplicate {
            participants: vec![],
            similarity: None,
        },
    }
}

pub(crate) fn make_structural_srp_finding(
    file: &str,
    line: usize,
) -> crate::domain::findings::SrpFinding {
    crate::domain::findings::SrpFinding {
        common: make_finding(
            file,
            line,
            crate::findings::Dimension::Srp,
            "srp/structural/SLM",
        ),
        kind: crate::domain::findings::SrpFindingKind::Structural,
        details: crate::domain::findings::SrpFindingDetails::Structural {
            item_name: "Foo::bar".into(),
            code: "SLM".into(),
            detail: "selfless method".into(),
        },
    }
}

pub(crate) fn make_structural_coupling_finding(
    file: &str,
    line: usize,
) -> crate::domain::findings::CouplingFinding {
    crate::domain::findings::CouplingFinding {
        common: make_finding(
            file,
            line,
            crate::findings::Dimension::Coupling,
            "coupling/structural/SIT",
        ),
        kind: crate::domain::findings::CouplingFindingKind::Structural,
        details: crate::domain::findings::CouplingFindingDetails::Structural {
            item_name: "Foo".into(),
            code: "SIT".into(),
            detail: "single-impl trait".into(),
        },
    }
}

pub(crate) fn make_architecture_finding(
    file: &str,
    line: usize,
) -> crate::domain::findings::ArchitectureFinding {
    crate::domain::findings::ArchitectureFinding {
        common: make_finding(
            file,
            line,
            crate::findings::Dimension::Architecture,
            "architecture::layer",
        ),
    }
}

/// Count orphan suppressions for a single marker at line 5 carrying `dims`,
/// against an analysis with an optional SRP-struct finding at `finding_line`.
pub(crate) fn srp_orphan_count(
    dims: &[crate::findings::Dimension],
    finding_line: Option<usize>,
) -> usize {
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![crate::findings::Suppression {
            line: 5,
            dimensions: dims.to_vec(),
            reason: None,
            target: None,
        }],
    );
    let mut analysis = empty_analysis();
    if let Some(line) = finding_line {
        analysis
            .findings
            .srp
            .push(make_srp_struct_finding("src/foo.rs", line));
    }
    crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &std::collections::HashMap::new(),
        &analysis,
        &Config::default(),
    )
    .len()
}

/// Like `make_fa_with_complexity` but flags the function as a test, so the
/// `is_test` short-circuits in `push_magic_numbers` / `exceeds_error_handling`
/// are exercised.
pub(crate) fn make_test_fa_with_complexity(
    file: &str,
    line: usize,
    metrics: crate::adapters::analyzers::iosp::ComplexityMetrics,
) -> FunctionAnalysis {
    let mut fa = make_fa_with_complexity(file, line, metrics);
    fa.is_test = true;
    fa
}

/// Run the orphan detector for a single `qual:allow(complexity)` marker placed
/// on the same line as `fa`, against an analysis whose only function is `fa`.
pub(crate) fn complexity_orphans_for(fa: FunctionAnalysis) -> usize {
    let mut sups = HashMap::new();
    sups.insert(
        fa.file.clone(),
        vec![crate::findings::Suppression {
            line: fa.line,
            dimensions: vec![crate::findings::Dimension::Complexity],
            reason: None,
            target: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis.results = vec![fa];
    crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &std::collections::HashMap::new(),
        &analysis,
        &Config::default(),
    )
    .len()
}
