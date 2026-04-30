//! Shared SRP projection: split `&[SrpFinding]` into typed buckets
//! (struct cohesion, module length, parameter count, structural).
//!
//! Reporter Views bundle these row types into per-reporter aggregator
//! structs (`text::views::SrpView`, `html::views::HtmlSrpView`).

use crate::domain::findings::{SrpFinding, SrpFindingDetails, SrpFindingKind};

/// Atomic struct-cohesion row, shared across reporters.
pub(crate) struct SrpStructRow {
    pub struct_name: String,
    pub file: String,
    pub line: usize,
    pub lcom4: usize,
    pub field_count: usize,
    pub method_count: usize,
    pub fan_out: usize,
}

/// Atomic module-length row.
pub(crate) struct SrpModuleRow {
    pub module: String,
    pub file: String,
    pub production_lines: usize,
    pub independent_clusters: usize,
    pub cluster_names: Vec<String>,
}

/// Atomic parameter-count row.
pub(crate) struct SrpParamRow {
    pub function_name: String,
    pub file: String,
    pub line: usize,
    pub parameter_count: usize,
}

/// Atomic structural-binary-check row (BTC/SLM/NMS for SRP, OI/SIT/
/// DEH/IET for Coupling — same shape).
pub(crate) struct StructuralRow {
    pub code: String,
    pub name: String,
    pub detail: String,
    pub file: String,
    pub line: usize,
}

/// All four SRP buckets, reporter-agnostic.
pub(crate) struct SrpBuckets {
    pub struct_warnings: Vec<SrpStructRow>,
    pub module_warnings: Vec<SrpModuleRow>,
    pub param_warnings: Vec<SrpParamRow>,
    pub structural_rows: Vec<StructuralRow>,
}

/// Project SRP findings into the four typed buckets. Filters out
/// suppressed findings.
pub(crate) fn split_srp_findings(findings: &[SrpFinding]) -> SrpBuckets {
    let mut buckets = SrpBuckets {
        struct_warnings: Vec::new(),
        module_warnings: Vec::new(),
        param_warnings: Vec::new(),
        structural_rows: Vec::new(),
    };
    findings
        .iter()
        .filter(|f| !f.common.suppressed)
        .for_each(|f| split_one(f, &mut buckets));
    buckets
}

fn split_one(f: &SrpFinding, buckets: &mut SrpBuckets) {
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
        ) => buckets.struct_warnings.push(SrpStructRow {
            struct_name: struct_name.clone(),
            file: f.common.file.clone(),
            line: f.common.line,
            lcom4: *lcom4,
            field_count: *field_count,
            method_count: *method_count,
            fan_out: *fan_out,
        }),
        (
            SrpFindingKind::ModuleLength,
            SrpFindingDetails::ModuleLength {
                module,
                production_lines,
                independent_clusters,
                cluster_names,
            },
        ) => buckets.module_warnings.push(SrpModuleRow {
            module: module.clone(),
            file: f.common.file.clone(),
            production_lines: *production_lines,
            independent_clusters: *independent_clusters,
            cluster_names: cluster_names.clone(),
        }),
        (
            SrpFindingKind::ParameterCount,
            SrpFindingDetails::ParameterCount {
                function_name,
                parameter_count,
            },
        ) => buckets.param_warnings.push(SrpParamRow {
            function_name: function_name.clone(),
            file: f.common.file.clone(),
            line: f.common.line,
            parameter_count: *parameter_count,
        }),
        (
            SrpFindingKind::Structural,
            SrpFindingDetails::Structural {
                item_name,
                code,
                detail,
            },
        ) => buckets.structural_rows.push(StructuralRow {
            code: code.clone(),
            name: item_name.clone(),
            detail: detail.clone(),
            file: f.common.file.clone(),
            line: f.common.line,
        }),
        _ => {}
    }
}
