//! Per-file function listings for verbose text output.
//!
//! Reads legacy `FunctionAnalysis` (the analyzer's per-function output
//! struct, not the typed `FunctionRecord`) — function records will
//! migrate as part of the Phase 9.5 cleanup. For now this module
//! preserves the existing verbose listing behaviour.

use std::fmt::Write;

use colored::Colorize;

use crate::adapters::analyzers::iosp::{Classification, FunctionAnalysis, Severity};

/// Format per-file function listings.
pub fn format_files_section(results: &[FunctionAnalysis], verbose: bool) -> String {
    if results.is_empty() {
        let mut out = String::new();
        let _ = writeln!(out, "{}", "No functions found to analyze.".yellow());
        return out;
    }
    let mut by_file: std::collections::BTreeMap<&str, Vec<&FunctionAnalysis>> =
        std::collections::BTreeMap::new();
    for r in results {
        by_file.entry(&r.file).or_default().push(r);
    }
    let mut out = String::new();
    for (file, functions) in &by_file {
        let has_violations = functions
            .iter()
            .any(|f| !f.suppressed && matches!(f.classification, Classification::Violation { .. }));
        let has_suppressed = functions.iter().any(|f| f.suppressed);
        if !verbose && !has_violations && !has_suppressed {
            continue;
        }
        let _ = writeln!(out, "\n{}", format!("── {} ", file).bold());
        functions
            .iter()
            .for_each(|func| push_function_entry(&mut out, func, verbose));
    }
    out
}

fn push_function_entry(out: &mut String, func: &FunctionAnalysis, verbose: bool) {
    let push_entry =
        |out: &mut String, tag: &dyn std::fmt::Display, name: &dyn std::fmt::Display| {
            let _ = writeln!(out, "  {} {} (line {})", tag, name, func.line);
        };

    if func.suppressed {
        if verbose {
            push_entry(
                out,
                &"~ SUPPRESSED ".yellow(),
                &func.qualified_name.dimmed(),
            );
        }
        return;
    }

    match &func.classification {
        Classification::Integration if verbose => {
            push_entry(out, &"✓ INTEGRATION".green(), &func.qualified_name.bold());
            push_complexity_details(out, func);
        }
        Classification::Operation if verbose => {
            push_entry(out, &"✓ OPERATION  ".blue(), &func.qualified_name.bold());
            push_complexity_details(out, func);
        }
        Classification::Trivial if verbose => {
            push_entry(
                out,
                &"· TRIVIAL    ".dimmed(),
                &func.qualified_name.dimmed(),
            );
            push_complexity_details(out, func);
        }
        Classification::Violation { .. } => {
            push_violation_detail(out, func);
            push_complexity_details(out, func);
        }
        _ => {}
    }
}

fn push_violation_detail(out: &mut String, func: &FunctionAnalysis) {
    let Classification::Violation {
        logic_locations,
        call_locations,
        ..
    } = &func.classification
    else {
        return;
    };

    let severity_tag = match &func.severity {
        Some(Severity::High) => " [HIGH]".red().bold().to_string(),
        Some(Severity::Medium) => " [MEDIUM]".yellow().to_string(),
        Some(Severity::Low) => " [LOW]".dimmed().to_string(),
        None => String::new(),
    };
    let _ = writeln!(
        out,
        "  {} {} (line {}){}",
        "✗ VIOLATION  ".red().bold(),
        func.qualified_name.bold(),
        func.line,
        severity_tag,
    );
    if !logic_locations.is_empty() {
        let logic_summary: Vec<String> = logic_locations.iter().map(|l| l.to_string()).collect();
        let _ = writeln!(
            out,
            "    {} {}",
            "Logic:".yellow(),
            logic_summary.join(", ")
        );
    }
    if !call_locations.is_empty() {
        let call_summary: Vec<String> = call_locations.iter().map(|c| c.to_string()).collect();
        let _ = writeln!(out, "    {} {}", "Calls:".yellow(), call_summary.join(", "));
    }
    if let Some(effort) = func.effort_score {
        let _ = writeln!(out, "    {} {:.1}", "Effort:".yellow(), effort);
    }
}

fn format_warning_messages(
    func: &FunctionAnalysis,
    m: &crate::adapters::analyzers::iosp::ComplexityMetrics,
) -> [Option<String>; 3] {
    let magic_msg = (!m.magic_numbers.is_empty()).then(|| {
        let nums: Vec<String> = m.magic_numbers.iter().map(|n| n.to_string()).collect();
        format!("magic numbers: {}", nums.join(", "))
    });
    let unsafe_msg = func.unsafe_warning.then(|| {
        let s = if m.unsafe_blocks == 1 { "" } else { "s" };
        format!("{} unsafe block{s}", m.unsafe_blocks)
    });
    let err_msg = func.error_handling_warning.then(|| {
        let parts: Vec<String> = [
            (m.unwrap_count, "unwrap"),
            (m.expect_count, "expect"),
            (m.panic_count, "panic/unreachable"),
            (m.todo_count, "todo"),
        ]
        .iter()
        .filter(|(c, _)| *c > 0)
        .map(|(c, l)| format!("{c} {l}"))
        .collect();
        format!("error handling: {}", parts.join(", "))
    });
    [magic_msg, unsafe_msg, err_msg]
}

fn push_complexity_details(out: &mut String, func: &FunctionAnalysis) {
    let Some(ref m) = func.complexity else { return };
    let warn = "⚠".yellow();

    if m.logic_count > 0 || m.call_count > 0 || m.max_nesting > 0 {
        let _ = writeln!(
            out,
            "    {} logic={}, calls={}, nesting={}, cognitive={}, cyclomatic={}",
            "Complexity:".dimmed(),
            m.logic_count,
            m.call_count,
            m.max_nesting,
            m.cognitive_complexity,
            m.cyclomatic_complexity,
        );
    }
    let [magic_msg, unsafe_msg, err_msg] = format_warning_messages(func, m);
    [
        func.cognitive_warning.then(|| {
            format!(
                "cognitive complexity {} exceeds threshold",
                m.cognitive_complexity
            )
        }),
        func.cyclomatic_warning.then(|| {
            format!(
                "cyclomatic complexity {} exceeds threshold",
                m.cyclomatic_complexity
            )
        }),
        magic_msg,
        func.nesting_depth_warning
            .then(|| format!("nesting depth {} exceeds threshold", m.max_nesting)),
        func.function_length_warning.then(|| {
            format!(
                "function length {} lines exceeds threshold",
                m.function_lines
            )
        }),
        unsafe_msg,
        err_msg,
    ]
    .iter()
    .flatten()
    .for_each(|w| {
        let _ = writeln!(out, "    {warn} {w}");
    });
}
