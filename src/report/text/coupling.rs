use colored::Colorize;

/// Print coupling analysis section with module metrics.
/// Integration: orchestrates coupling sub-sections.
pub fn print_coupling_section(
    analysis: &crate::coupling::CouplingAnalysis,
    config: &crate::config::sections::CouplingConfig,
    verbose: bool,
) {
    print_coupling_header(analysis, verbose);
    print_coupling_cycles(analysis);
    print_coupling_sdp_violations(analysis);
    print_coupling_table(analysis, config, verbose);
}

/// Print coupling section header with module count.
/// Operation: formatting logic, no own calls.
fn print_coupling_header(analysis: &crate::coupling::CouplingAnalysis, verbose: bool) {
    println!("\n{}", "═══ Coupling ═══".bold());
    if verbose {
        println!("  Modules analyzed: {}", analysis.metrics.len());
    }
}

/// Print circular dependency cycles.
/// Operation: iteration and formatting logic, no own calls.
fn print_coupling_cycles(analysis: &crate::coupling::CouplingAnalysis) {
    if !analysis.cycles.is_empty() {
        for cycle in &analysis.cycles {
            println!(
                "  {} Circular dependency: {}",
                "✗".red(),
                cycle.modules.join(" → "),
            );
        }
    }
}

/// Print SDP violations, skipping suppressed ones.
/// Operation: iteration and formatting logic, no own calls.
fn print_coupling_sdp_violations(analysis: &crate::coupling::CouplingAnalysis) {
    if analysis.sdp_violations.is_empty() {
        return;
    }
    analysis
        .sdp_violations
        .iter()
        .filter(|v| !v.suppressed)
        .for_each(|v| {
            println!(
                "  {} SDP violation: {} (I={:.2}) depends on {} (I={:.2})",
                "⚠".yellow(),
                v.from_module,
                v.from_instability,
                v.to_module,
                v.to_instability,
            );
        });
}

/// Print module coupling metrics table.
/// Integration: orchestrates legend, table rows, and cycle status.
fn print_coupling_table(
    analysis: &crate::coupling::CouplingAnalysis,
    config: &crate::config::sections::CouplingConfig,
    verbose: bool,
) {
    if verbose {
        print_coupling_legend(&analysis.metrics);
    } else if !analysis.metrics.is_empty() {
        println!("\n    {:<20} {:>3}  {:>3}  Instability", "", "In", "Out");
    }
    print_coupling_rows(&analysis.metrics, config, verbose);
    print_coupling_cycle_status(&analysis.cycles);
}

/// Print coupling table legend and column headers.
/// Operation: formatting logic, no own calls.
fn print_coupling_legend(metrics: &[crate::coupling::CouplingMetrics]) {
    if !metrics.is_empty() {
        println!(
            "\n  {} {}\n  {} {}\n  {} {}",
            "Incoming".dimmed(),
            "= modules depending on this one".dimmed(),
            "Outgoing".dimmed(),
            "= modules this one depends on".dimmed(),
            "Instability".dimmed(),
            "= Outgoing / (Incoming + Outgoing)".dimmed(),
        );
        println!("\n    {:<20} {:>3}  {:>3}  Instability", "", "In", "Out",);
    }
}

/// Print individual module coupling rows with threshold tags.
/// Operation: iteration and formatting logic, no own calls.
/// Threshold check logic is in a closure (lenient mode).
fn print_coupling_rows(
    metrics: &[crate::coupling::CouplingMetrics],
    config: &crate::config::sections::CouplingConfig,
    verbose: bool,
) {
    let module_tag = |m: &crate::coupling::CouplingMetrics| -> String {
        if m.suppressed {
            return format!("  {}", "~ suppressed".yellow());
        }
        let instability_exceeded = m.afferent > 0 && m.instability > config.max_instability;
        let fan_in_exceeded = m.afferent > config.max_fan_in;
        let fan_out_exceeded = m.efferent > config.max_fan_out;
        let has_warning = instability_exceeded || fan_in_exceeded || fan_out_exceeded;
        if has_warning {
            format!("  {} exceeds threshold", "⚠".yellow())
        } else {
            String::new()
        }
    };

    metrics.iter().for_each(|m| {
        println!(
            "    {:<20} {:>3}  {:>3}  {:.2}{}",
            m.module_name,
            m.afferent,
            m.efferent,
            m.instability,
            module_tag(m),
        );
        if verbose && !m.outgoing.is_empty() {
            println!(
                "      {} {}",
                "→ depends on:".dimmed(),
                m.outgoing.join(", "),
            );
        }
        if verbose && !m.incoming.is_empty() {
            println!("      {} {}", "← used by:".dimmed(), m.incoming.join(", "),);
        }
    });
}

/// Print no-cycles confirmation.
/// Operation: conditional formatting, no own calls.
fn print_coupling_cycle_status(cycles: &[crate::coupling::CycleReport]) {
    if cycles.is_empty() {
        println!("\n  {} No circular dependencies.", "✓".green());
    }
}
