use colored::Colorize;

/// Print SRP analysis section — Integration: delegates to header + per-category printers.
pub fn print_srp_section(srp: &crate::srp::SrpAnalysis) {
    print_srp_header(srp);
    print_srp_struct_warnings(&srp.struct_warnings);
    print_srp_module_warnings(&srp.module_warnings);
    print_srp_param_warnings(&srp.param_warnings);
}

/// Print the SRP section header if there are any unsuppressed warnings.
/// Operation: conditional formatting logic, no own calls.
fn print_srp_header(srp: &crate::srp::SrpAnalysis) {
    let has_any = srp.struct_warnings.iter().any(|w| !w.suppressed)
        || srp.module_warnings.iter().any(|w| !w.suppressed)
        || srp.param_warnings.iter().any(|w| !w.suppressed);
    if has_any {
        println!("\n{}", "═══ SRP Analysis ═══".bold());
    }
}

/// Print SRP struct cohesion warnings.
/// Operation: conditional formatting logic, no own calls.
fn print_srp_struct_warnings(warnings: &[crate::srp::SrpWarning]) {
    for w in warnings {
        if w.suppressed {
            continue;
        }
        println!(
            "  {} {} ({}:{}) — LCOM4={}, fields={}, methods={}, fan-out={}, score={:.2}",
            "⚠".yellow(),
            w.struct_name,
            w.file,
            w.line,
            w.lcom4,
            w.field_count,
            w.method_count,
            w.fan_out,
            w.composite_score,
        );
        for (i, cluster) in w.clusters.iter().enumerate() {
            println!(
                "    Cluster {}: methods=[{}], fields=[{}]",
                i + 1,
                cluster.methods.join(", "),
                cluster.fields.join(", "),
            );
        }
    }
}

/// Print SRP module length warnings.
/// Operation: conditional formatting logic, no own calls.
fn print_srp_module_warnings(warnings: &[crate::srp::ModuleSrpWarning]) {
    for w in warnings {
        if w.suppressed {
            continue;
        }
        if w.length_score > 0.0 {
            println!(
                "  {} {} — {} production lines (score={:.2})",
                "⚠".yellow(),
                w.module,
                w.production_lines,
                w.length_score,
            );
        }
        if w.independent_clusters > 0 {
            println!(
                "  {} {} — {} independent function clusters",
                "⚠".yellow(),
                w.module,
                w.independent_clusters,
            );
            for (i, cluster) in w.cluster_names.iter().enumerate() {
                println!("    Cluster {}: [{}]", i + 1, cluster.join(", "));
            }
        }
    }
}

/// Print SRP too-many-arguments warnings.
/// Operation: conditional formatting logic, no own calls.
fn print_srp_param_warnings(warnings: &[crate::srp::ParamSrpWarning]) {
    for w in warnings {
        if w.suppressed {
            continue;
        }
        println!(
            "  {} {} ({}:{}) — {} parameters (exceeds threshold)",
            "⚠".yellow(),
            w.function_name,
            w.file,
            w.line,
            w.parameter_count,
        );
    }
}
