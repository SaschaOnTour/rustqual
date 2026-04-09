use crate::analyzer::{Classification, FunctionAnalysis};
use crate::report::AnalysisResult;

/// A single finding with its location, category, and detail.
#[derive(Debug, Clone)]
pub struct FindingEntry {
    pub file: String,
    pub line: usize,
    pub category: &'static str,
    pub detail: String,
    pub function_name: String,
}

impl FindingEntry {
    fn new(
        file: &str,
        line: usize,
        category: &'static str,
        detail: String,
        context: String,
    ) -> Self {
        Self {
            file: file.to_string(),
            line,
            category,
            detail,
            function_name: context,
        }
    }
}

/// Collect all findings from an analysis result into a flat, sorted list.
/// Integration: orchestrates per-dimension collectors via closures.
pub fn collect_all_findings(analysis: &AnalysisResult) -> Vec<FindingEntry> {
    let mut entries = Vec::new();
    collect_function_findings(&analysis.results, &mut entries);
    collect_dry_findings(analysis, &mut entries);
    collect_srp_findings(analysis, &mut entries);
    collect_coupling_findings(analysis, &mut entries);
    collect_tq_findings(analysis, &mut entries);
    collect_structural_findings(analysis, &mut entries);
    entries.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    entries
}

/// Print findings in one-line-per-finding format.
/// Operation: formatting logic, no own calls.
pub fn print_findings(entries: &[FindingEntry]) {
    if entries.is_empty() {
        println!("No findings.");
        return;
    }
    entries.iter().for_each(|e| {
        println!(
            "{}:{}  {}  {}  in {}",
            e.file, e.line, e.category, e.detail, e.function_name
        );
    });
}

// ── Per-dimension collectors (Operations) ───────────────────────

/// Collect IOSP violation and complexity findings from function results.
/// Operation: iterates function results checking flags; no own calls.
fn collect_function_findings(results: &[FunctionAnalysis], entries: &mut Vec<FindingEntry>) {
    results.iter().filter(|f| !f.suppressed).for_each(|f| {
        let e = |cat, detail: String| {
            FindingEntry::new(&f.file, f.line, cat, detail, f.qualified_name.clone())
        };
        if matches!(f.classification, Classification::Violation { .. }) {
            entries.push(e("VIOLATION", "logic + calls".to_string()));
        }
        if f.cognitive_warning {
            let cx = f.complexity.as_ref().map_or(0, |m| m.cognitive_complexity);
            entries.push(e("COGNITIVE", format!("complexity {cx}")));
        }
        if f.cyclomatic_warning {
            let cx = f.complexity.as_ref().map_or(0, |m| m.cyclomatic_complexity);
            entries.push(e("CYCLOMATIC", format!("complexity {cx}")));
        }
        if let Some(ref m) = f.complexity {
            m.magic_numbers.iter().for_each(|mn| {
                entries.push(FindingEntry::new(
                    &f.file,
                    mn.line,
                    "MAGIC_NUMBER",
                    mn.value.clone(),
                    f.qualified_name.clone(),
                ));
            });
        }
        if f.nesting_depth_warning {
            let depth = f.complexity.as_ref().map_or(0, |m| m.max_nesting);
            entries.push(e("NESTING", format!("depth {depth}")));
        }
        if f.function_length_warning {
            let lines = f.complexity.as_ref().map_or(0, |m| m.function_lines);
            entries.push(e("LONG_FN", format!("{lines} lines")));
        }
        if f.unsafe_warning {
            let blocks = f.complexity.as_ref().map_or(0, |m| m.unsafe_blocks);
            entries.push(e("UNSAFE", format!("{blocks} blocks")));
        }
        if f.error_handling_warning {
            entries.push(e("ERROR_HANDLING", "unwrap/panic/todo".to_string()));
        }
    });
}

/// Collect DRY findings (duplicates, dead code, fragments, boilerplate, wildcards).
/// Operation: iterates DRY analysis results; no own calls.
fn collect_dry_findings(analysis: &AnalysisResult, entries: &mut Vec<FindingEntry>) {
    analysis
        .duplicates
        .iter()
        .filter(|g| !g.suppressed)
        .for_each(|group| {
            let kind = match &group.kind {
                crate::dry::DuplicateKind::Exact => "exact".to_string(),
                crate::dry::DuplicateKind::NearDuplicate { similarity } => {
                    format!("{:.0}% similar", similarity * 100.0)
                }
            };
            group.entries.iter().for_each(|e| {
                entries.push(FindingEntry::new(
                    &e.file,
                    e.line,
                    "DUPLICATE",
                    kind.clone(),
                    e.qualified_name.clone(),
                ));
            });
        });
    analysis.dead_code.iter().for_each(|w| {
        entries.push(FindingEntry::new(
            &w.file,
            w.line,
            "DEAD_CODE",
            format!("{:?}", w.kind).to_lowercase(),
            w.qualified_name.clone(),
        ));
    });
    analysis
        .fragments
        .iter()
        .filter(|g| !g.suppressed)
        .for_each(|group| {
            group.entries.iter().for_each(|e| {
                entries.push(FindingEntry::new(
                    &e.file,
                    e.start_line,
                    "FRAGMENT",
                    format!("{} stmts", group.statement_count),
                    e.function_name.clone(),
                ));
            });
        });
    analysis
        .boilerplate
        .iter()
        .filter(|b| !b.suppressed)
        .for_each(|b| {
            entries.push(FindingEntry::new(
                &b.file,
                b.line,
                "BOILERPLATE",
                b.pattern_id.clone(),
                b.struct_name.clone().unwrap_or_default(),
            ));
        });
    analysis
        .wildcard_warnings
        .iter()
        .filter(|w| !w.suppressed)
        .for_each(|w| {
            entries.push(FindingEntry::new(
                &w.file,
                w.line,
                "WILDCARD",
                w.module_path.clone(),
                String::new(),
            ));
        });
    analysis
        .repeated_matches
        .iter()
        .filter(|g| !g.suppressed)
        .for_each(|group| {
            group.entries.iter().for_each(|e| {
                entries.push(FindingEntry::new(
                    &e.file,
                    e.line,
                    "REPEATED_MATCH",
                    group.enum_name.clone(),
                    e.function_name.clone(),
                ));
            });
        });
}

/// Collect SRP findings.
/// Operation: iterates SRP warnings; no own calls.
fn collect_srp_findings(analysis: &AnalysisResult, entries: &mut Vec<FindingEntry>) {
    let srp = match &analysis.srp {
        Some(s) => s,
        None => return,
    };
    srp.struct_warnings
        .iter()
        .filter(|w| !w.suppressed)
        .for_each(|w| {
            entries.push(FindingEntry::new(
                &w.file,
                w.line,
                "SRP_STRUCT",
                format!("LCOM4={}", w.lcom4),
                w.struct_name.clone(),
            ));
        });
    srp.module_warnings
        .iter()
        .filter(|w| !w.suppressed)
        .for_each(|w| {
            entries.push(FindingEntry::new(
                &w.file,
                1,
                "SRP_MODULE",
                format!("{} lines", w.production_lines),
                w.module.clone(),
            ));
        });
    srp.param_warnings
        .iter()
        .filter(|w| !w.suppressed)
        .for_each(|w| {
            entries.push(FindingEntry::new(
                &w.file,
                w.line,
                "SRP_PARAMS",
                format!("{} params", w.parameter_count),
                w.function_name.clone(),
            ));
        });
}

/// Collect coupling findings (SDP violations).
/// Operation: iterates coupling analysis; no own calls.
fn collect_coupling_findings(analysis: &AnalysisResult, entries: &mut Vec<FindingEntry>) {
    let ca = match &analysis.coupling {
        Some(c) => c,
        None => return,
    };
    ca.sdp_violations
        .iter()
        .filter(|v| !v.suppressed)
        .for_each(|v| {
            entries.push(FindingEntry::new(
                "",
                0,
                "SDP",
                format!("{} -> {}", v.from_module, v.to_module),
                v.from_module.clone(),
            ));
        });
}

/// Collect TQ findings.
/// Operation: iterates TQ warnings; no own calls.
fn collect_tq_findings(analysis: &AnalysisResult, entries: &mut Vec<FindingEntry>) {
    let tq = match &analysis.tq {
        Some(t) => t,
        None => return,
    };
    tq.warnings.iter().filter(|w| !w.suppressed).for_each(|w| {
        let cat = match &w.kind {
            crate::tq::TqWarningKind::NoAssertion => "TQ_NO_ASSERT",
            crate::tq::TqWarningKind::NoSut => "TQ_NO_SUT",
            crate::tq::TqWarningKind::Untested => "TQ_UNTESTED",
            crate::tq::TqWarningKind::Uncovered => "TQ_UNCOVERED",
            crate::tq::TqWarningKind::UntestedLogic { .. } => "TQ_UNTESTED_LOGIC",
        };
        entries.push(FindingEntry::new(
            &w.file,
            w.line,
            cat,
            String::new(),
            w.function_name.clone(),
        ));
    });
}

/// Collect structural findings.
/// Operation: iterates structural warnings; no own calls.
fn collect_structural_findings(analysis: &AnalysisResult, entries: &mut Vec<FindingEntry>) {
    let st = match &analysis.structural {
        Some(s) => s,
        None => return,
    };
    st.warnings.iter().filter(|w| !w.suppressed).for_each(|w| {
        entries.push(FindingEntry::new(
            &w.file,
            w.line,
            "STRUCTURAL",
            w.kind.code().to_string(),
            w.name.clone(),
        ));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::{
        Classification, ComplexityMetrics, FunctionAnalysis, MagicNumberOccurrence,
    };
    use crate::report::{AnalysisResult, Summary};

    fn make_fa(name: &str, file: &str, line: usize) -> FunctionAnalysis {
        FunctionAnalysis {
            name: name.to_string(),
            file: file.to_string(),
            line,
            classification: Classification::Operation,
            parent_type: None,
            suppressed: false,
            complexity: None,
            qualified_name: name.to_string(),
            severity: None,
            cognitive_warning: false,
            cyclomatic_warning: false,
            nesting_depth_warning: false,
            function_length_warning: false,
            unsafe_warning: false,
            error_handling_warning: false,
            complexity_suppressed: false,
            own_calls: vec![],
            parameter_count: 0,
            is_trait_impl: false,
            is_test: false,
            effort_score: None,
        }
    }

    fn empty_analysis() -> AnalysisResult {
        AnalysisResult {
            results: vec![],
            summary: Summary::default(),
            coupling: None,
            duplicates: vec![],
            dead_code: vec![],
            fragments: vec![],
            boilerplate: vec![],
            wildcard_warnings: vec![],
            repeated_matches: vec![],
            srp: None,
            tq: None,
            structural: None,
        }
    }

    #[test]
    fn test_collect_empty_analysis() {
        let analysis = empty_analysis();
        let findings = collect_all_findings(&analysis);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_collect_magic_numbers() {
        let mut analysis = empty_analysis();
        let mut fa = make_fa("test_fn", "src/lib.rs", 10);
        fa.complexity = Some(ComplexityMetrics {
            magic_numbers: vec![
                MagicNumberOccurrence {
                    line: 12,
                    value: "42".to_string(),
                },
                MagicNumberOccurrence {
                    line: 15,
                    value: "99".to_string(),
                },
            ],
            ..Default::default()
        });
        analysis.results = vec![fa];
        let findings = collect_all_findings(&analysis);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].category, "MAGIC_NUMBER");
        assert_eq!(findings[0].detail, "42");
        assert_eq!(findings[1].detail, "99");
    }

    #[test]
    fn test_collect_violation() {
        let mut analysis = empty_analysis();
        let mut fa = make_fa("bad_fn", "src/lib.rs", 5);
        fa.classification = Classification::Violation {
            has_logic: true,
            has_own_calls: true,
            logic_locations: vec![],
            call_locations: vec![],
        };
        analysis.results = vec![fa];
        let findings = collect_all_findings(&analysis);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].category, "VIOLATION");
    }

    #[test]
    fn test_sorted_by_file_and_line() {
        let mut analysis = empty_analysis();
        let mut fa1 = make_fa("fn_b", "src/b.rs", 20);
        fa1.error_handling_warning = true;
        fa1.complexity = Some(ComplexityMetrics::default());
        let mut fa2 = make_fa("fn_a", "src/a.rs", 10);
        fa2.error_handling_warning = true;
        fa2.complexity = Some(ComplexityMetrics::default());
        analysis.results = vec![fa1, fa2];
        let findings = collect_all_findings(&analysis);
        assert_eq!(findings[0].file, "src/a.rs");
        assert_eq!(findings[1].file, "src/b.rs");
    }

    #[test]
    fn test_suppressed_not_collected() {
        let mut analysis = empty_analysis();
        let mut fa = make_fa("suppressed_fn", "src/lib.rs", 5);
        fa.suppressed = true;
        fa.classification = Classification::Violation {
            has_logic: true,
            has_own_calls: true,
            logic_locations: vec![],
            call_locations: vec![],
        };
        analysis.results = vec![fa];
        let findings = collect_all_findings(&analysis);
        assert!(findings.is_empty());
    }
}
