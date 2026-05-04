//! Shared Coupling projection: split `&[CouplingFinding]` into typed
//! buckets (cycles, SDP violations, structural).

use crate::domain::findings::{CouplingFinding, CouplingFindingDetails};

use super::srp::StructuralRow;

/// Atomic SDP-violation row.
pub(crate) struct SdpViolationRow {
    pub from: String,
    pub from_instability: f64,
    pub to: String,
    pub to_instability: f64,
}

/// All three Coupling-finding buckets, reporter-agnostic.
pub(crate) struct CouplingBuckets {
    /// Each cycle is a path of module names.
    pub cycle_paths: Vec<Vec<String>>,
    pub sdp_violations: Vec<SdpViolationRow>,
    pub structural_rows: Vec<StructuralRow>,
}

/// Project Coupling findings into the three typed buckets. Cycles are
/// included regardless of suppression (cycles are global module-level
/// reports). SDP and Structural are filtered by `!suppressed`.
pub(crate) fn split_coupling_findings(findings: &[CouplingFinding]) -> CouplingBuckets {
    let mut buckets = CouplingBuckets {
        cycle_paths: Vec::new(),
        sdp_violations: Vec::new(),
        structural_rows: Vec::new(),
    };
    findings.iter().for_each(|f| split_one(f, &mut buckets));
    buckets
}

fn split_one(f: &CouplingFinding, buckets: &mut CouplingBuckets) {
    match &f.details {
        CouplingFindingDetails::Cycle { modules } => {
            buckets.cycle_paths.push(modules.clone());
        }
        CouplingFindingDetails::SdpViolation {
            from_module,
            to_module,
            from_instability,
            to_instability,
        } if !f.common.suppressed => {
            buckets.sdp_violations.push(SdpViolationRow {
                from: from_module.clone(),
                from_instability: *from_instability,
                to: to_module.clone(),
                to_instability: *to_instability,
            });
        }
        CouplingFindingDetails::Structural {
            item_name,
            code,
            detail,
        } if !f.common.suppressed => {
            buckets.structural_rows.push(StructuralRow {
                code: code.clone(),
                name: item_name.clone(),
                detail: detail.clone(),
                file: f.common.file.clone(),
                line: f.common.line,
            });
        }
        _ => {}
    }
}
