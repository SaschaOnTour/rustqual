use std::fmt::Write;

use colored::Colorize;

use crate::domain::PERCENTAGE_MULTIPLIER;
use crate::report::findings_list::FindingEntry;
use crate::report::Summary;

/// Maximum number of total findings before inline locations are hidden in the summary.
const INLINE_LOCATION_THRESHOLD: usize = 10;

/// Format summary statistics and final status message.
/// Integration: orchestrates per-section summary builders.
pub(super) fn format_summary_section(summary: &Summary, findings: &[FindingEntry]) -> String {
    let mut out = String::new();
    push_summary_header(&mut out, summary);
    push_dimension_scores(&mut out, summary, findings);
    push_summary_suppression(&mut out, summary);
    push_summary_footer(&mut out, summary);
    out
}

/// Append the summary header: function count, quality score, IOSP detail.
fn push_summary_header(out: &mut String, summary: &Summary) {
    let pct = |v: f64| v * PERCENTAGE_MULTIPLIER;
    let _ = writeln!(out);
    let _ = writeln!(out, "{}", "═══ Summary ═══".bold());
    let total_findings = summary.total_findings();
    if total_findings > 0 {
        let _ = writeln!(
            out,
            "  Functions: {}    Quality Score: {:.1}%    {} finding{}",
            summary.total,
            pct(summary.quality_score),
            total_findings,
            if total_findings == 1 { "" } else { "s" }
        );
    } else {
        let _ = writeln!(
            out,
            "  Functions: {}    Quality Score: {:.1}%",
            summary.total,
            pct(summary.quality_score)
        );
    }
    let _ = writeln!(out);
    let s = summary;
    let iosp_detail = if s.violations > 0 {
        let pl = if s.violations == 1 { "" } else { "s" };
        format!(
            "{}I, {}O, {}T, {} violation{pl}",
            s.integrations, s.operations, s.trivial, s.violations
        )
    } else {
        format!("{}I, {}O, {}T", s.integrations, s.operations, s.trivial)
    };
    let _ = writeln!(
        out,
        "  {:<13}{:>5.1}%  ({})",
        "IOSP:",
        pct(s.dimension_scores[0]),
        iosp_detail
    );
}

/// A dimension entry: (display name, score, list of (count, label) finding categories).
type DimensionEntry = (&'static str, f64, Vec<(usize, &'static str)>);

/// Build the dimension data array for score printing.
/// Operation: array construction logic.
fn build_dimensions(s: &Summary) -> Vec<DimensionEntry> {
    let [_, cx, dry, srp, cp, tq, arch] = s.dimension_scores;
    vec![
        (
            "Complexity",
            cx,
            vec![
                (s.complexity_warnings, "complexity"),
                (s.magic_number_warnings, "magic numbers"),
                (s.nesting_depth_warnings, "nesting"),
                (s.function_length_warnings, "long fn"),
                (s.unsafe_warnings, "unsafe"),
                (s.error_handling_warnings, "error handling"),
            ],
        ),
        (
            "DRY",
            dry,
            vec![
                (s.duplicate_groups, "duplicates"),
                (s.fragment_groups, "fragments"),
                (s.dead_code_warnings, "dead code"),
                (s.boilerplate_warnings, "boilerplate"),
                (s.wildcard_import_warnings, "wildcards"),
                (s.repeated_match_groups, "repeated match"),
            ],
        ),
        (
            "SRP",
            srp,
            vec![
                (s.srp_struct_warnings, "struct"),
                (s.srp_module_warnings, "module"),
                (s.srp_param_warnings, "params"),
                (s.structural_srp_warnings, "structural"),
            ],
        ),
        (
            "Coupling",
            cp,
            vec![
                (s.coupling_warnings, "instability"),
                (s.coupling_cycles, "cycles"),
                (s.sdp_violations, "SDP"),
                (s.structural_coupling_warnings, "structural"),
            ],
        ),
        (
            "Test Quality",
            tq,
            vec![
                (s.tq_no_assertion_warnings, "no assertion"),
                (s.tq_no_sut_warnings, "no SUT"),
                (s.tq_untested_warnings, "untested"),
                (s.tq_uncovered_warnings, "uncovered"),
                (s.tq_untested_logic_warnings, "untested logic"),
            ],
        ),
        (
            "Architecture",
            arch,
            vec![(s.architecture_warnings, "architecture")],
        ),
    ]
}

/// Append per-dimension score lines with optional inline finding locations.
fn push_dimension_scores(out: &mut String, summary: &Summary, findings: &[FindingEntry]) {
    let pct = |v: f64| v * PERCENTAGE_MULTIPLIER;
    let show_locs =
        |s: &Summary| s.total_findings() <= INLINE_LOCATION_THRESHOLD && !findings.is_empty();
    let should_show = show_locs(summary);
    let dims = build_dimensions(summary);

    dims.iter().for_each(|(name, score, dim_findings)| {
        let d: Vec<String> = dim_findings
            .iter()
            .filter(|(c, _)| *c > 0)
            .map(|(c, l)| format!("{c} {l}"))
            .collect();
        let label = format!("{name}:");
        if d.is_empty() {
            let _ = writeln!(out, "  {:<13}{:>5.1}%", label, pct(*score));
        } else {
            let _ = writeln!(
                out,
                "  {:<13}{:>5.1}%  ({})",
                label,
                pct(*score),
                d.join(", ")
            );
            if should_show {
                push_inline_locations(out, name, findings);
            }
        }
    });
}

/// Append `→ file:line (detail)` sub-lines for findings in a given dimension.
fn push_inline_locations(out: &mut String, dim_name: &str, findings: &[FindingEntry]) {
    let dim_cats = dimension_categories(dim_name);
    findings
        .iter()
        .filter(|f| dim_cats.contains(&f.category) && !f.file.is_empty())
        .for_each(|f| {
            let loc = if f.detail.is_empty() {
                f.function_name.clone()
            } else {
                format!("{} — {}", f.function_name, f.detail)
            };
            let _ = writeln!(out, "    {} {}:{} ({})", "→".dimmed(), f.file, f.line, loc);
        });
}

/// Map dimension display name to finding categories.
fn dimension_categories(dim_name: &str) -> &[&str] {
    match dim_name {
        "Complexity" => &[
            "COGNITIVE",
            "CYCLOMATIC",
            "MAGIC_NUMBER",
            "NESTING",
            "LONG_FN",
            "UNSAFE",
            "ERROR_HANDLING",
        ],
        "DRY" => &[
            "DUPLICATE",
            "DEAD_CODE",
            "FRAGMENT",
            "BOILERPLATE",
            "WILDCARD",
            "REPEATED_MATCH",
        ],
        "SRP" => &["SRP_STRUCT", "SRP_MODULE", "SRP_PARAMS", "SRP_STRUCTURAL"],
        "Coupling" => &["COUPLING", "CYCLE", "SDP", "COUPLING_STRUCTURAL"],
        "Test Quality" => &[
            "TQ_NO_ASSERT",
            "TQ_NO_SUT",
            "TQ_UNTESTED",
            "TQ_UNCOVERED",
            "TQ_UNTESTED_LOGIC",
        ],
        "Architecture" => &["ARCHITECTURE"],
        _ => &[],
    }
}

/// Append suppression info if any functions are suppressed.
fn push_summary_suppression(out: &mut String, summary: &Summary) {
    if summary.suppressed > 0 || summary.all_suppressions > 0 {
        let _ = writeln!(out);
    }
    if summary.suppressed > 0 {
        let _ = writeln!(
            out,
            "  {} Suppressed:   {}",
            "~".yellow(),
            summary.suppressed
        );
    }
    if summary.all_suppressions > 0 {
        let _ = writeln!(
            out,
            "  {} All allows:   {} (qual:allow + #[allow])",
            "~".yellow(),
            summary.all_suppressions
        );
        if summary.suppression_ratio_exceeded {
            let _ = writeln!(
                out,
                "  {} Suppression ratio exceeds configured maximum",
                "⚠".yellow()
            );
        }
    }
}

/// Append dimension-neutral footer message.
fn push_summary_footer(out: &mut String, summary: &Summary) {
    let total = summary.total_findings();
    if total == 0 {
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "{}",
            "All quality checks passed! \u{2713}".green().bold()
        );
    }
}
