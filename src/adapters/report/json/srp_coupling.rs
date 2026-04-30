//! Atomic per-section builders for SRP and Coupling, plus structural
//! warnings folding. Each builder takes only what it needs from typed
//! Findings/Data so the JsonReporter trait methods can compose them.

use super::super::json_types::{
    JsonCouplingModule, JsonModuleSrpWarning, JsonParamSrpWarning, JsonSdpViolation,
    JsonSrpWarning, JsonStructuralWarning,
};
use crate::domain::analysis_data::ModuleCouplingRecord;
use crate::domain::findings::{
    CouplingFinding, CouplingFindingDetails, SrpFinding, SrpFindingDetails, SrpFindingKind,
};

pub(super) fn build_coupling_modules(modules: &[ModuleCouplingRecord]) -> Vec<JsonCouplingModule> {
    modules
        .iter()
        .map(|m| JsonCouplingModule {
            name: m.module_name.clone(),
            afferent: m.afferent,
            efferent: m.efferent,
            instability: m.instability,
        })
        .collect()
}

pub(super) fn build_cycles(findings: &[CouplingFinding]) -> Vec<Vec<String>> {
    findings
        .iter()
        .filter_map(|f| match &f.details {
            CouplingFindingDetails::Cycle { modules } => Some(modules.clone()),
            _ => None,
        })
        .collect()
}

pub(super) fn build_sdp_violations(findings: &[CouplingFinding]) -> Vec<JsonSdpViolation> {
    findings
        .iter()
        .filter(|f| !f.common.suppressed)
        .filter_map(|f| match &f.details {
            CouplingFindingDetails::SdpViolation {
                from_module,
                to_module,
                from_instability,
                to_instability,
            } => Some(JsonSdpViolation {
                from_module: from_module.clone(),
                to_module: to_module.clone(),
                from_instability: *from_instability,
                to_instability: *to_instability,
            }),
            _ => None,
        })
        .collect()
}

#[allow(clippy::type_complexity)]
pub(super) fn build_srp_lists(
    findings: &[SrpFinding],
) -> (
    Vec<JsonSrpWarning>,
    Vec<JsonModuleSrpWarning>,
    Vec<JsonParamSrpWarning>,
) {
    let mut struct_warnings = Vec::new();
    let mut module_warnings = Vec::new();
    let mut param_warnings = Vec::new();
    findings
        .iter()
        .filter(|f| !f.common.suppressed)
        .for_each(|f| {
            collect_srp_into(
                f,
                &mut struct_warnings,
                &mut module_warnings,
                &mut param_warnings,
            );
        });
    (struct_warnings, module_warnings, param_warnings)
}

fn collect_srp_into(
    f: &SrpFinding,
    structs: &mut Vec<JsonSrpWarning>,
    modules: &mut Vec<JsonModuleSrpWarning>,
    params: &mut Vec<JsonParamSrpWarning>,
) {
    match (&f.kind, &f.details) {
        (
            SrpFindingKind::StructCohesion,
            SrpFindingDetails::StructCohesion {
                struct_name,
                lcom4,
                field_count,
                method_count,
                fan_out,
            },
        ) => structs.push(JsonSrpWarning {
            struct_name: struct_name.clone(),
            file: f.common.file.clone(),
            line: f.common.line,
            lcom4: *lcom4,
            field_count: *field_count,
            method_count: *method_count,
            fan_out: *fan_out,
            composite_score: 0.0,
            clusters: vec![],
        }),
        (
            SrpFindingKind::ModuleLength,
            SrpFindingDetails::ModuleLength {
                module,
                production_lines,
                independent_clusters,
                cluster_names,
            },
        ) => modules.push(JsonModuleSrpWarning {
            module: module.clone(),
            file: f.common.file.clone(),
            production_lines: *production_lines,
            length_score: 0.0,
            independent_clusters: *independent_clusters,
            cluster_names: cluster_names.iter().map(|s| vec![s.clone()]).collect(),
        }),
        (
            SrpFindingKind::ParameterCount,
            SrpFindingDetails::ParameterCount {
                function_name,
                parameter_count,
            },
        ) => params.push(JsonParamSrpWarning {
            function_name: function_name.clone(),
            file: f.common.file.clone(),
            line: f.common.line,
            parameter_count: *parameter_count,
        }),
        _ => {}
    }
}

pub(super) fn build_structural(
    srp_findings: &[SrpFinding],
    coupling_findings: &[CouplingFinding],
) -> Vec<JsonStructuralWarning> {
    let srp_rows = srp_findings.iter().filter_map(|f| {
        if let SrpFindingDetails::Structural {
            item_name,
            code,
            detail,
        } = &f.details
        {
            Some(make_row(&f.common, item_name, code, detail, "srp"))
        } else {
            None
        }
    });
    let coupling_rows = coupling_findings.iter().filter_map(|f| {
        if let CouplingFindingDetails::Structural {
            item_name,
            code,
            detail,
        } = &f.details
        {
            Some(make_row(&f.common, item_name, code, detail, "coupling"))
        } else {
            None
        }
    });
    srp_rows.chain(coupling_rows).collect()
}

fn make_row(
    common: &crate::domain::Finding,
    item_name: &str,
    code: &str,
    detail: &str,
    dim: &str,
) -> JsonStructuralWarning {
    JsonStructuralWarning {
        file: common.file.clone(),
        line: common.line,
        name: item_name.to_string(),
        code: code.to_string(),
        detail: detail.to_string(),
        dimension: dim.to_string(),
    }
}
