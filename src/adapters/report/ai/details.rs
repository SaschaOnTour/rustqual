//! Per-dim detail formatters: take a typed Finding's `details` enum
//! and produce a `(category, detail)` pair for the AI envelope.

use crate::config::Config;
use crate::domain::findings::{
    CouplingFinding, CouplingFindingDetails, DryFinding, DryFindingDetails, SrpFinding,
    SrpFindingDetails,
};

pub(super) fn dry_category_detail(f: &DryFinding) -> (&'static str, String) {
    match &f.details {
        DryFindingDetails::Duplicate { participants } => {
            let partners: Vec<String> = participants
                .iter()
                .filter(|p| !(p.file == f.common.file && p.line == f.common.line))
                .map(|p| format!("{}:{}", p.file, p.line))
                .collect();
            let detail = if partners.is_empty() {
                "exact".to_string()
            } else {
                format!("exact with {}", partners.join(", "))
            };
            ("duplicate", detail)
        }
        DryFindingDetails::Fragment {
            participants,
            statement_count,
        } => {
            let partners: Vec<String> = participants
                .iter()
                .filter(|p| !(p.file == f.common.file && p.line == f.common.line))
                .map(|p| format!("{}:{}", p.file, p.line))
                .collect();
            let detail = if partners.is_empty() {
                format!("{statement_count} stmts")
            } else {
                format!("{statement_count} stmts also in {}", partners.join(", "))
            };
            ("fragment", detail)
        }
        DryFindingDetails::DeadCode {
            qualified_name,
            suggestion,
        } => {
            let s = suggestion.clone().unwrap_or_default();
            let detail = if s.is_empty() {
                qualified_name.clone()
            } else {
                format!("{qualified_name} ({s})")
            };
            ("dead_code", detail)
        }
        DryFindingDetails::Wildcard { module_path } => ("wildcard_import", module_path.clone()),
        DryFindingDetails::Boilerplate {
            pattern_id,
            suggestion,
            ..
        } => (
            "boilerplate",
            format!("{pattern_id}: {} — {suggestion}", f.common.message),
        ),
        DryFindingDetails::RepeatedMatch { enum_name, .. } => ("repeated_match", enum_name.clone()),
    }
}

pub(super) fn srp_category_detail(f: &SrpFinding, config: &Config) -> (&'static str, String) {
    match &f.details {
        SrpFindingDetails::StructCohesion {
            struct_name,
            lcom4,
            method_count,
            ..
        } => (
            "srp_struct",
            format!("{struct_name}: LCOM4={lcom4}, methods={method_count}"),
        ),
        SrpFindingDetails::ModuleLength {
            module,
            production_lines,
            independent_clusters,
            ..
        } => {
            let length_part = format!(
                "{production_lines} lines (max {})",
                config.srp.file_length_baseline
            );
            let cluster_part = if *independent_clusters > config.srp.max_independent_clusters {
                format!(
                    ", {independent_clusters} independent clusters (max {})",
                    config.srp.max_independent_clusters
                )
            } else {
                String::new()
            };
            (
                "srp_module",
                format!("{module}: {length_part}{cluster_part}"),
            )
        }
        SrpFindingDetails::ParameterCount {
            function_name,
            parameter_count,
        } => (
            "srp_params",
            format!(
                "{function_name}: {parameter_count} (max {})",
                config.srp.max_parameters
            ),
        ),
        SrpFindingDetails::Structural { code, detail, .. } => {
            ("structural", format!("{code}: {detail}"))
        }
    }
}

pub(super) fn coupling_category_detail(f: &CouplingFinding) -> (&'static str, String) {
    match &f.details {
        CouplingFindingDetails::Cycle { modules } => ("cycle", modules.join(" -> ")),
        CouplingFindingDetails::SdpViolation {
            from_module,
            to_module,
            from_instability,
            to_instability,
        } => (
            "sdp_violation",
            format!(
                "{from_module} -> {to_module} (stable I={from_instability:.2} imports unstable I={to_instability:.2})"
            ),
        ),
        CouplingFindingDetails::ThresholdExceeded {
            module_name,
            instability,
            ..
        } => ("coupling", format!("{module_name}: I={instability:.2}")),
        CouplingFindingDetails::Structural { code, detail, .. } => {
            ("structural", format!("{code}: {detail}"))
        }
    }
}
