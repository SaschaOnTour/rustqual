// qual:allow(srp) reason: "aggregates all per-dimension collectors in one file; split scheduled for Phase 9"
use crate::adapters::analyzers::iosp::{Classification, FunctionAnalysis};
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
    collect_architecture_findings(analysis, &mut entries);
    entries.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    entries
}

/// Collect Architecture-dimension findings from the port-based analyzers.
/// Operation: iterator-chain projection of Finding into FindingEntry.
fn collect_architecture_findings(analysis: &AnalysisResult, entries: &mut Vec<FindingEntry>) {
    entries.extend(
        analysis
            .architecture_findings
            .iter()
            .filter(|f| !f.suppressed)
            .map(|f| {
                FindingEntry::new(
                    &f.file,
                    f.line,
                    "ARCHITECTURE",
                    f.message.clone(),
                    String::new(),
                )
            }),
    );
}

/// Print findings in one-line-per-finding format with heading.
/// Operation: formatting logic, no own calls.
pub fn print_findings(entries: &[FindingEntry]) {
    if entries.is_empty() {
        return;
    }
    let n = entries.len();
    let heading = format!("═══ {} Finding{} ═══", n, if n == 1 { "" } else { "s" });
    println!("\n{}", colored::Colorize::bold(heading.as_str()));
    entries.iter().for_each(|e| {
        let detail = if e.function_name.is_empty() {
            e.detail.clone()
        } else if e.detail.is_empty() {
            format!("in {}", e.function_name)
        } else {
            format!("{}  in {}", e.detail, e.function_name)
        };
        if e.file.is_empty() {
            println!("  {}  {}", e.category, detail);
        } else {
            println!("  {}:{}  {}  {}", e.file, e.line, e.category, detail);
        }
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
                crate::adapters::analyzers::dry::DuplicateKind::Exact => "exact".to_string(),
                crate::adapters::analyzers::dry::DuplicateKind::NearDuplicate { similarity } => {
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
        let detail = format!("{:?}", w.kind).to_lowercase();
        entries.push(FindingEntry::new(
            &w.file,
            w.line,
            "DEAD_CODE",
            detail,
            w.qualified_name.clone(),
        ));
    });
    analysis
        .fragments
        .iter()
        .filter(|g| !g.suppressed)
        .for_each(|group| {
            group.entries.iter().for_each(|e| {
                let detail = format!("{} stmts", group.statement_count);
                entries.push(FindingEntry::new(
                    &e.file,
                    e.start_line,
                    "FRAGMENT",
                    detail,
                    e.function_name.clone(),
                ));
            });
        });
    analysis
        .boilerplate
        .iter()
        .filter(|b| !b.suppressed)
        .for_each(|b| {
            let name = b.struct_name.clone().unwrap_or_default();
            entries.push(FindingEntry::new(
                &b.file,
                b.line,
                "BOILERPLATE",
                b.pattern_id.clone(),
                name,
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
    let Some(srp) = &analysis.srp else { return };
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

/// Collect coupling findings (threshold warnings, cycles, SDP violations).
/// Operation: iterates coupling analysis; no own calls.
fn collect_coupling_findings(analysis: &AnalysisResult, entries: &mut Vec<FindingEntry>) {
    let Some(ca) = &analysis.coupling else { return };
    ca.metrics.iter().filter(|m| m.warning).for_each(|m| {
        let detail = format!("I={:.2} Ca={} Ce={}", m.instability, m.afferent, m.efferent);
        entries.push(FindingEntry::new(
            "",
            0,
            "COUPLING",
            detail,
            m.module_name.clone(),
        ));
    });
    ca.cycles.iter().for_each(|c| {
        let detail = c.modules.join(" > ");
        entries.push(FindingEntry::new("", 0, "CYCLE", detail, String::new()));
    });
    ca.sdp_violations
        .iter()
        .filter(|v| !v.suppressed)
        .for_each(|v| {
            let detail = format!("{} -> {}", v.from_module, v.to_module);
            entries.push(FindingEntry::new(
                "",
                0,
                "SDP",
                detail,
                v.from_module.clone(),
            ));
        });
}

/// Collect TQ findings.
/// Operation: iterates TQ warnings; no own calls.
fn collect_tq_findings(analysis: &AnalysisResult, entries: &mut Vec<FindingEntry>) {
    let Some(tq) = &analysis.tq else { return };
    tq.warnings.iter().filter(|w| !w.suppressed).for_each(|w| {
        let cat = match &w.kind {
            crate::adapters::analyzers::tq::TqWarningKind::NoAssertion => "TQ_NO_ASSERT",
            crate::adapters::analyzers::tq::TqWarningKind::NoSut => "TQ_NO_SUT",
            crate::adapters::analyzers::tq::TqWarningKind::Untested => "TQ_UNTESTED",
            crate::adapters::analyzers::tq::TqWarningKind::Uncovered => "TQ_UNCOVERED",
            crate::adapters::analyzers::tq::TqWarningKind::UntestedLogic { .. } => {
                "TQ_UNTESTED_LOGIC"
            }
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
    let Some(st) = &analysis.structural else {
        return;
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
