//! Per-dimension category-and-detail mappings used by the
//! findings-list reporter. Pure projections from typed Findings into
//! the (category, detail) pair that one-line-per-finding output expects.

use crate::domain::findings::{
    ComplexityFinding, ComplexityFindingKind, CouplingFinding, CouplingFindingDetails, DryFinding,
    DryFindingDetails, DryFindingKind, SrpFinding, SrpFindingDetails, SrpFindingKind,
    TqFindingKind,
};

pub(super) fn complexity_category(kind: ComplexityFindingKind) -> &'static str {
    match kind {
        ComplexityFindingKind::Cognitive => "COGNITIVE",
        ComplexityFindingKind::Cyclomatic => "CYCLOMATIC",
        ComplexityFindingKind::NestingDepth => "NESTING",
        ComplexityFindingKind::FunctionLength => "LONG_FN",
        ComplexityFindingKind::MagicNumber => "MAGIC_NUMBER",
        ComplexityFindingKind::Unsafe => "UNSAFE",
        ComplexityFindingKind::ErrorHandling => "ERROR_HANDLING",
    }
}

pub(super) fn complexity_detail(f: &ComplexityFinding) -> String {
    match f.kind {
        ComplexityFindingKind::Cognitive | ComplexityFindingKind::Cyclomatic => {
            format!("complexity {}", f.metric_value)
        }
        ComplexityFindingKind::NestingDepth => format!("depth {}", f.metric_value),
        ComplexityFindingKind::FunctionLength => format!("{} lines", f.metric_value),
        ComplexityFindingKind::MagicNumber => f
            .common
            .message
            .split_whitespace()
            .nth(2)
            .unwrap_or(&f.common.message)
            .to_string(),
        ComplexityFindingKind::Unsafe => format!("{} blocks", f.metric_value),
        ComplexityFindingKind::ErrorHandling => "unwrap/panic/todo".into(),
    }
}

pub(super) fn dry_category_detail(f: &DryFinding) -> (&'static str, String) {
    match (f.kind, &f.details) {
        (DryFindingKind::DuplicateExact, DryFindingDetails::Duplicate { .. }) => {
            ("DUPLICATE", "exact".into())
        }
        (DryFindingKind::DuplicateSimilar, DryFindingDetails::Duplicate { .. }) => {
            ("DUPLICATE", "similar".into())
        }
        (
            DryFindingKind::Fragment,
            DryFindingDetails::Fragment {
                statement_count, ..
            },
        ) => ("FRAGMENT", format!("{statement_count} stmts")),
        (DryFindingKind::DeadCodeUncalled, DryFindingDetails::DeadCode { qualified_name, .. }) => {
            ("DEAD_CODE", qualified_name.clone())
        }
        (DryFindingKind::DeadCodeTestOnly, DryFindingDetails::DeadCode { qualified_name, .. }) => {
            ("DEAD_CODE", format!("testonly {qualified_name}"))
        }
        (DryFindingKind::Wildcard, DryFindingDetails::Wildcard { module_path }) => {
            ("WILDCARD", module_path.clone())
        }
        (DryFindingKind::Boilerplate, DryFindingDetails::Boilerplate { pattern_id, .. }) => {
            ("BOILERPLATE", pattern_id.clone())
        }
        (DryFindingKind::RepeatedMatch, DryFindingDetails::RepeatedMatch { enum_name, .. }) => {
            ("REPEATED_MATCH", enum_name.clone())
        }
        _ => ("UNKNOWN", f.common.message.clone()),
    }
}

pub(super) fn srp_category_detail(f: &SrpFinding) -> (&'static str, String) {
    match (&f.kind, &f.details) {
        (
            SrpFindingKind::StructCohesion,
            SrpFindingDetails::StructCohesion {
                struct_name, lcom4, ..
            },
        ) => ("SRP_STRUCT", format!("{struct_name}: LCOM4={lcom4}")),
        (
            SrpFindingKind::ModuleLength,
            SrpFindingDetails::ModuleLength {
                production_lines, ..
            },
        ) => ("SRP_MODULE", format!("{production_lines} lines")),
        (
            SrpFindingKind::ParameterCount,
            SrpFindingDetails::ParameterCount {
                parameter_count, ..
            },
        ) => ("SRP_PARAMS", format!("{parameter_count} params")),
        (SrpFindingKind::Structural, SrpFindingDetails::Structural { code, .. }) => {
            ("SRP_STRUCTURAL", code.clone())
        }
        _ => ("UNKNOWN", f.common.message.clone()),
    }
}

pub(super) fn coupling_category_detail(f: &CouplingFinding) -> (&'static str, String) {
    match &f.details {
        CouplingFindingDetails::Cycle { modules } => ("CYCLE", modules.join(" -> ")),
        CouplingFindingDetails::SdpViolation {
            from_module,
            to_module,
            ..
        } => ("SDP", format!("{from_module} -> {to_module}")),
        CouplingFindingDetails::ThresholdExceeded {
            module_name,
            instability,
            ..
        } => ("COUPLING", format!("{module_name} I={instability:.2}")),
        CouplingFindingDetails::Structural { code, .. } => ("COUPLING_STRUCTURAL", code.clone()),
    }
}

pub(super) fn tq_category(kind: &TqFindingKind) -> &'static str {
    kind.meta().findings_list_category
}
