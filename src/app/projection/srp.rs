//! SRP-dimension projection: struct cohesion + module length + parameter
//! count → typed `Vec<SrpFinding>`.

use crate::adapters::analyzers::srp::{ModuleSrpWarning, ParamSrpWarning, SrpAnalysis, SrpWarning};
use crate::adapters::analyzers::structural::{StructuralAnalysis, StructuralWarning};
use crate::domain::findings::{SrpFinding, SrpFindingDetails, SrpFindingKind};
use crate::domain::{Dimension, Finding, Severity};

const DIM: Dimension = Dimension::Srp;
const SEV: Severity = Severity::Medium;

/// Project SRP analyzer output into typed SrpFinding entries.
///
/// Includes both the SRP-native warnings (struct cohesion, module
/// length, parameter count) and the structural binary checks that
/// belong to SRP (BTC/SLM/NMS).
pub(crate) fn project_srp(
    srp: Option<&SrpAnalysis>,
    structural: Option<&StructuralAnalysis>,
) -> Vec<SrpFinding> {
    let mut out = Vec::new();
    if let Some(srp) = srp {
        out.extend(srp.struct_warnings.iter().map(project_struct));
        out.extend(srp.module_warnings.iter().map(project_module));
        out.extend(srp.param_warnings.iter().map(project_param));
    }
    if let Some(s) = structural {
        s.warnings
            .iter()
            .filter(|w| w.dimension == Dimension::Srp)
            .for_each(|w| out.push(project_structural(w)));
    }
    out
}

fn project_struct(w: &SrpWarning) -> SrpFinding {
    SrpFinding {
        common: Finding {
            file: w.file.clone(),
            line: w.line,
            column: 0,
            dimension: DIM,
            rule_id: "srp/struct_cohesion".into(),
            message: format!(
                "{}: low cohesion (LCOM4={}, fields={}, methods={})",
                w.struct_name, w.lcom4, w.field_count, w.method_count
            ),
            severity: SEV,
            suppressed: w.suppressed,
        },
        kind: SrpFindingKind::StructCohesion,
        details: SrpFindingDetails::StructCohesion {
            struct_name: w.struct_name.clone(),
            lcom4: w.lcom4,
            field_count: w.field_count,
            method_count: w.method_count,
            fan_out: w.fan_out,
        },
    }
}

fn project_module(w: &ModuleSrpWarning) -> SrpFinding {
    let cluster_names: Vec<String> = w
        .cluster_names
        .iter()
        .map(|cluster| cluster.join(", "))
        .collect();
    SrpFinding {
        common: Finding {
            file: w.file.clone(),
            line: 1,
            column: 0,
            dimension: DIM,
            rule_id: "srp/module_length".into(),
            message: format!(
                "{}: {} production lines, {} independent clusters",
                w.module, w.production_lines, w.independent_clusters
            ),
            severity: SEV,
            suppressed: w.suppressed,
        },
        kind: SrpFindingKind::ModuleLength,
        details: SrpFindingDetails::ModuleLength {
            module: w.module.clone(),
            production_lines: w.production_lines,
            independent_clusters: w.independent_clusters,
            cluster_names,
        },
    }
}

fn project_param(w: &ParamSrpWarning) -> SrpFinding {
    SrpFinding {
        common: Finding {
            file: w.file.clone(),
            line: w.line,
            column: 0,
            dimension: DIM,
            rule_id: "srp/parameter_count".into(),
            message: format!("{}: {} parameters", w.function_name, w.parameter_count),
            severity: SEV,
            suppressed: w.suppressed,
        },
        kind: SrpFindingKind::ParameterCount,
        details: SrpFindingDetails::ParameterCount {
            function_name: w.function_name.clone(),
            parameter_count: w.parameter_count,
        },
    }
}

fn project_structural(w: &StructuralWarning) -> SrpFinding {
    let p = super::structural_shared::structural_pieces(w, DIM);
    SrpFinding {
        common: p.common,
        kind: SrpFindingKind::Structural,
        details: SrpFindingDetails::Structural {
            item_name: w.name.clone(),
            code: p.code,
            detail: p.detail,
        },
    }
}
