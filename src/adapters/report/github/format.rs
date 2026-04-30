//! Format functions: convert per-dim view rows into
//! `::level file=,line=::msg\n` annotation lines for GitHub Actions.

use super::views::{
    GithubArchitectureView, GithubComplexityView, GithubCouplingRow, GithubCouplingView,
    GithubDetailListView, GithubDetailRow, GithubDryView, GithubIospView, GithubSrpRow,
    GithubSrpView, GithubTqView,
};
use crate::domain::findings::{
    ComplexityFindingKind, CouplingFindingDetails, DryFindingDetails, SrpFindingDetails,
};
use crate::domain::Severity;

pub(crate) fn format_iosp(view: &GithubIospView) -> String {
    let mut out = String::new();
    view.rows.iter().for_each(|r| {
        let logic: Vec<String> = r
            .logic_locations
            .iter()
            .map(|(k, ln)| format!("{k} (line {ln})"))
            .collect();
        let calls: Vec<String> = r
            .call_locations
            .iter()
            .map(|(n, ln)| format!("{n} (line {ln})"))
            .collect();
        let effort = r
            .effort_score
            .map(|e| format!(", effort={e:.1}"))
            .unwrap_or_default();
        let msg = format!(
            "IOSP violation: logic=[{}], calls=[{}]{effort}",
            logic.join(", "),
            calls.join(", "),
        );
        out.push_str(&located(level_of(&r.severity), &r.file, r.line, &msg));
    });
    out
}

pub(crate) fn format_complexity(view: &GithubComplexityView) -> String {
    let mut out = String::new();
    view.rows.iter().for_each(|r| {
        out.push_str(&located(
            complexity_level(r.kind),
            &r.file,
            r.line,
            &r.message,
        ));
    });
    out
}

pub(crate) fn format_dry(view: &GithubDryView) -> String {
    format_detail_view(view, format_dry_message)
}

pub(crate) fn format_srp(view: &GithubSrpView) -> String {
    format_detail_view(view, format_srp_message)
}

pub(crate) fn format_coupling(view: &GithubCouplingView) -> String {
    format_detail_view(view, format_coupling_message)
}

fn format_detail_view<D>(
    view: &GithubDetailListView<D>,
    msg: impl Fn(&GithubDetailRow<D>) -> String,
) -> String {
    let mut out = String::new();
    view.rows.iter().for_each(|r| {
        out.push_str(&located(level_of(&r.severity), &r.file, r.line, &msg(r)));
    });
    out
}

fn format_dry_message(r: &GithubDetailRow<DryFindingDetails>) -> String {
    match &r.details {
        DryFindingDetails::Duplicate { participants } => {
            let names: Vec<&str> = participants
                .iter()
                .map(|p| p.function_name.as_str())
                .collect();
            format!("Duplicate functions: {}", names.join(", "))
        }
        DryFindingDetails::Fragment {
            participants,
            statement_count,
        } => {
            let names: Vec<&str> = participants
                .iter()
                .map(|p| p.function_name.as_str())
                .collect();
            format!(
                "Duplicate fragment ({statement_count} stmts): {}",
                names.join(", ")
            )
        }
        _ => r.fallback_message.clone(),
    }
}

fn format_srp_message(r: &GithubSrpRow) -> String {
    match &r.details {
        SrpFindingDetails::StructCohesion {
            struct_name,
            lcom4,
            method_count,
            ..
        } => {
            format!("SRP cohesion: {struct_name} has LCOM4={lcom4}, methods={method_count}")
        }
        SrpFindingDetails::ModuleLength {
            module,
            production_lines,
            independent_clusters,
            ..
        } => {
            format!(
                "SRP module length: {module} has {production_lines} lines, {independent_clusters} independent clusters"
            )
        }
        SrpFindingDetails::ParameterCount {
            function_name,
            parameter_count,
        } => {
            format!("SRP params: '{function_name}' has {parameter_count} parameters — reduce parameter count")
        }
        _ => r.fallback_message.clone(),
    }
}

fn format_coupling_message(r: &GithubCouplingRow) -> String {
    match &r.details {
        CouplingFindingDetails::Cycle { modules } => {
            format!("Coupling cycle: {}", modules.join(" \u{2192} "))
        }
        CouplingFindingDetails::SdpViolation {
            from_module,
            to_module,
            from_instability,
            to_instability,
        } => {
            format!(
                "SDP violation: {from_module} (I={from_instability:.2}) depends on {to_module} (I={to_instability:.2})"
            )
        }
        CouplingFindingDetails::ThresholdExceeded {
            module_name,
            instability,
            ..
        } => {
            format!("Coupling threshold exceeded: {module_name} (I={instability:.2})")
        }
        _ => r.fallback_message.clone(),
    }
}

pub(crate) fn format_tq(view: &GithubTqView) -> String {
    let mut out = String::new();
    view.rows.iter().for_each(|r| {
        out.push_str(&located(level_of(&r.severity), &r.file, r.line, &r.message));
    });
    out
}

pub(crate) fn format_architecture(view: &GithubArchitectureView) -> String {
    let mut out = String::new();
    view.rows.iter().for_each(|r| {
        let msg = format!("{} \u{2014} {}", r.rule_id, r.message);
        out.push_str(&located(level_of(&r.severity), &r.file, r.line, &msg));
    });
    out
}

fn level_of(s: &Severity) -> &'static str {
    s.levels().github
}

fn complexity_level(kind: ComplexityFindingKind) -> &'static str {
    // Threshold breaches are notices; smell findings (magic numbers,
    // unsafe, error handling) are warnings. Severity stays Medium
    // across the board.
    match kind {
        ComplexityFindingKind::Cognitive
        | ComplexityFindingKind::Cyclomatic
        | ComplexityFindingKind::NestingDepth
        | ComplexityFindingKind::FunctionLength => "notice",
        ComplexityFindingKind::MagicNumber
        | ComplexityFindingKind::Unsafe
        | ComplexityFindingKind::ErrorHandling => "warning",
    }
}

fn located(level: &str, file: &str, line: usize, msg: &str) -> String {
    if file.is_empty() || line == 0 {
        format!("::{level}::{msg}\n")
    } else {
        format!("::{level} file={file},line={line}::{msg}\n")
    }
}
