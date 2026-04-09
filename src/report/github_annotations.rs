/// Print `::notice` annotations for DRY/boilerplate/wildcard/repeated-match findings.
/// Operation: iteration + formatting logic, no own calls.
pub fn print_dry_annotations(analysis: &super::AnalysisResult) {
    for g in analysis.duplicates.iter().filter(|g| !g.suppressed) {
        let names: Vec<&str> = g
            .entries
            .iter()
            .map(|e| e.qualified_name.as_str())
            .collect();
        println!("::notice::Duplicate functions: {}", names.join(", "),);
    }
    for w in &analysis.dead_code {
        println!(
            "::notice file={},line={}::Dead code: {} — {}",
            w.file, w.line, w.qualified_name, w.suggestion,
        );
    }
    for g in analysis.fragments.iter().filter(|g| !g.suppressed) {
        let names: Vec<&str> = g
            .entries
            .iter()
            .map(|e| e.qualified_name.as_str())
            .collect();
        println!(
            "::notice::Duplicate fragment ({} stmts): {}",
            g.statement_count,
            names.join(", "),
        );
    }
    for b in analysis.boilerplate.iter().filter(|b| !b.suppressed) {
        println!(
            "::notice file={},line={}::{} — {}",
            b.file, b.line, b.description, b.suggestion,
        );
    }
    for w in &analysis.wildcard_warnings {
        if w.suppressed {
            continue;
        }
        println!(
            "::notice file={},line={}::Wildcard import: {}",
            w.file, w.line, w.module_path,
        );
    }
    for g in analysis.repeated_matches.iter().filter(|g| !g.suppressed) {
        let fns: Vec<&str> = g.entries.iter().map(|e| e.function_name.as_str()).collect();
        println!(
            "::notice::DRY-005: Repeated match on '{}' in: {}",
            g.enum_name,
            fns.join(", "),
        );
    }
}

/// Print `::error` annotations for circular module dependencies.
/// Operation: iteration + formatting logic, no own calls.
/// Leaf modules (afferent=0) are excluded from instability warnings.
pub fn print_coupling_annotations(
    analysis: &crate::coupling::CouplingAnalysis,
    config: &crate::config::sections::CouplingConfig,
) {
    for cycle in &analysis.cycles {
        println!(
            "::error::Circular module dependency: {}",
            cycle.modules.join(" → "),
        );
    }
    for m in &analysis.metrics {
        if m.suppressed {
            continue;
        }
        if m.afferent > 0 && m.instability > config.max_instability {
            println!(
                "::warning::Module '{}' has high instability ({:.2})",
                m.module_name, m.instability,
            );
        }
    }
    for v in &analysis.sdp_violations {
        if v.suppressed {
            continue;
        }
        println!(
            "::warning::SDP violation: '{}' (I={:.2}) depends on '{}' (I={:.2})",
            v.from_module, v.from_instability, v.to_module, v.to_instability,
        );
    }
}

/// Print `::warning` annotations for TQ findings.
/// Operation: iteration + formatting logic, no own calls.
pub fn print_tq_annotations(tq: &crate::tq::TqAnalysis) {
    for w in &tq.warnings {
        if w.suppressed {
            continue;
        }
        let kind_label = match &w.kind {
            crate::tq::TqWarningKind::NoAssertion => "TQ-001: test has no assertions".to_string(),
            crate::tq::TqWarningKind::NoSut => {
                "TQ-002: test does not call production code".to_string()
            }
            crate::tq::TqWarningKind::Untested => {
                "TQ-003: production function is untested".to_string()
            }
            crate::tq::TqWarningKind::Uncovered => {
                "TQ-004: production function has no coverage".to_string()
            }
            crate::tq::TqWarningKind::UntestedLogic { uncovered_lines } => {
                let lines: Vec<String> = uncovered_lines
                    .iter()
                    .map(|(f, l)| format!("{f}:{l}"))
                    .collect();
                format!("TQ-005: untested logic at {}", lines.join(", "))
            }
        };
        println!(
            "::warning file={},line={}::{} in '{}'",
            w.file, w.line, kind_label, w.function_name,
        );
    }
}

/// Print `::warning` annotations for structural findings.
/// Trivial: iteration with method calls hidden in closure (lenient mode).
pub fn print_structural_annotations(structural: &crate::structural::StructuralAnalysis) {
    structural
        .warnings
        .iter()
        .filter(|w| !w.suppressed)
        .for_each(|w| {
            let (code, detail) = (w.kind.code(), w.kind.detail());
            println!(
                "::warning file={},line={}::{code}: '{}' — {detail}",
                w.file, w.line, w.name,
            );
        });
}

/// Print `::warning` annotations for SRP findings.
/// Operation: iteration + formatting logic, no own calls.
pub fn print_srp_annotations(srp: &crate::srp::SrpAnalysis) {
    for w in &srp.struct_warnings {
        if w.suppressed {
            continue;
        }
        println!(
            "::warning file={},line={}::SRP warning: {} has LCOM4={}, score={:.2}",
            w.file, w.line, w.struct_name, w.lcom4, w.composite_score,
        );
    }
    for w in &srp.module_warnings {
        if w.suppressed {
            continue;
        }
        if w.length_score > 0.0 {
            println!(
                "::warning file={}::Module has {} production lines (score={:.2})",
                w.file, w.production_lines, w.length_score,
            );
        }
        if w.independent_clusters > 0 {
            println!(
                "::warning file={}::Module has {} independent function clusters",
                w.file, w.independent_clusters,
            );
        }
    }
    for w in &srp.param_warnings {
        if w.suppressed {
            continue;
        }
        println!(
            "::warning file={},line={}::Function '{}' has {} parameters — reduce parameter count",
            w.file, w.line, w.function_name, w.parameter_count,
        );
    }
}
