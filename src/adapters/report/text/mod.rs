mod coupling;
mod dry;
mod srp;
pub(crate) mod structural;
mod summary;
pub(crate) mod tq;

pub use coupling::print_coupling_section;
pub use dry::print_dry_section;
pub use srp::print_srp_section;
pub(crate) use structural::print_structural_section;
pub(crate) use tq::print_tq_section;

use colored::Colorize;

use crate::adapters::analyzers::iosp::{Classification, FunctionAnalysis, Severity};

use super::Summary;

/// Print summary section (scores, dimension breakdown, suppression info).
/// Integration: delegates to summary section printer.
pub fn print_summary_only(
    summary: &Summary,
    findings: &[crate::report::findings_list::FindingEntry],
) {
    summary::print_summary_section(summary, findings);
}

/// Print only the file-grouped function listings (verbose mode).
/// Trivial: delegates to print_files_section.
pub fn print_files_only(results: &[FunctionAnalysis]) {
    print_files_section(results, true);
}

/// Print per-file function listings.
/// Operation: file grouping and iteration logic; delegates per-function
/// printing via `.for_each` closure (no own calls in lenient mode).
fn print_files_section(results: &[FunctionAnalysis], verbose: bool) {
    if results.is_empty() {
        println!("{}", "No functions found to analyze.".yellow());
        return;
    }

    let mut by_file: std::collections::BTreeMap<&str, Vec<&FunctionAnalysis>> =
        std::collections::BTreeMap::new();
    for r in results {
        by_file.entry(&r.file).or_default().push(r);
    }

    for (file, functions) in &by_file {
        let has_violations = functions
            .iter()
            .any(|f| !f.suppressed && matches!(f.classification, Classification::Violation { .. }));
        let has_suppressed = functions.iter().any(|f| f.suppressed);

        if !verbose && !has_violations && !has_suppressed {
            continue;
        }

        println!("\n{}", format!("── {} ", file).bold());
        functions
            .iter()
            .for_each(|func| print_function_entry(func, verbose));
    }
}

/// Print a single function entry based on its classification.
/// Operation: classification dispatch logic; helper calls hidden in closures.
fn print_function_entry(func: &FunctionAnalysis, verbose: bool) {
    let (show_violation, show_complexity) = (
        |f: &FunctionAnalysis| print_violation_detail(f),
        |f: &FunctionAnalysis| print_complexity_details(f),
    );
    let print_entry = |tag: &dyn std::fmt::Display, name: &dyn std::fmt::Display| {
        println!("  {} {} (line {})", tag, name, func.line);
    };

    if func.suppressed {
        if verbose {
            print_entry(&"~ SUPPRESSED ".yellow(), &func.qualified_name.dimmed());
        }
        return;
    }

    match &func.classification {
        Classification::Integration if verbose => {
            print_entry(&"✓ INTEGRATION".green(), &func.qualified_name.bold());
            show_complexity(func);
        }
        Classification::Operation if verbose => {
            print_entry(&"✓ OPERATION  ".blue(), &func.qualified_name.bold());
            show_complexity(func);
        }
        Classification::Trivial if verbose => {
            print_entry(&"· TRIVIAL    ".dimmed(), &func.qualified_name.dimmed());
            show_complexity(func);
        }
        Classification::Violation { .. } => {
            show_violation(func);
            show_complexity(func);
        }
        _ => {}
    }
}

/// Print violation details: severity tag, logic locations, call locations.
/// Operation: formatting logic, no own calls.
fn print_violation_detail(func: &FunctionAnalysis) {
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
    println!(
        "  {} {} (line {}){}",
        "✗ VIOLATION  ".red().bold(),
        func.qualified_name.bold(),
        func.line,
        severity_tag,
    );
    if !logic_locations.is_empty() {
        let logic_summary: Vec<String> = logic_locations.iter().map(|l| l.to_string()).collect();
        println!("    {} {}", "Logic:".yellow(), logic_summary.join(", "));
    }
    if !call_locations.is_empty() {
        let call_summary: Vec<String> = call_locations.iter().map(|c| c.to_string()).collect();
        println!("    {} {}", "Calls:".yellow(), call_summary.join(", "));
    }
    if let Some(effort) = func.effort_score {
        println!("    {} {:.1}", "Effort:".yellow(), effort);
    }
}

/// Build optional warning messages for magic numbers, unsafe blocks, and error handling.
/// Operation: conditional formatting logic, no own calls.
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

/// Print complexity metrics for a function.
/// Operation: data-driven formatting logic; helper call hidden in closure.
fn print_complexity_details(func: &FunctionAnalysis) {
    let Some(ref m) = func.complexity else { return };
    let warn = "⚠".yellow();
    let msgs = |f: &FunctionAnalysis,
                metrics: &crate::adapters::analyzers::iosp::ComplexityMetrics| {
        format_warning_messages(f, metrics)
    };

    if m.logic_count > 0 || m.call_count > 0 || m.max_nesting > 0 {
        println!(
            "    {} logic={}, calls={}, nesting={}, cognitive={}, cyclomatic={}",
            "Complexity:".dimmed(),
            m.logic_count,
            m.call_count,
            m.max_nesting,
            m.cognitive_complexity,
            m.cyclomatic_complexity,
        );
    }
    let [magic_msg, unsafe_msg, err_msg] = msgs(func, m);
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
    .for_each(|w| println!("    {warn} {w}"));
}

#[cfg(test)]
mod tests;
