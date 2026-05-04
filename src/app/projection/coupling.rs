//! Coupling-dimension projection: cycles, SDP violations, threshold
//! breaches → typed `Vec<CouplingFinding>`.

use crate::adapters::analyzers::coupling::sdp::SdpViolation;
use crate::adapters::analyzers::coupling::{CouplingAnalysis, CouplingMetrics, CycleReport};
use crate::adapters::analyzers::structural::{StructuralAnalysis, StructuralWarning};
use crate::domain::findings::{CouplingFinding, CouplingFindingDetails, CouplingFindingKind};
use crate::domain::{Dimension, Finding, Severity};

const DIM: Dimension = Dimension::Coupling;
const SEV: Severity = Severity::Medium;

/// Project Coupling analyzer output into typed CouplingFinding entries.
///
/// Includes both the coupling-native findings (cycles, SDP violations,
/// threshold breaches) and the structural binary checks that belong
/// to Coupling (OI/SIT/DEH/IET).
pub(crate) fn project_coupling(
    coupling: Option<&CouplingAnalysis>,
    structural: Option<&StructuralAnalysis>,
) -> Vec<CouplingFinding> {
    let mut out = Vec::new();
    if let Some(c) = coupling {
        out.extend(c.cycles.iter().map(project_cycle));
        out.extend(c.sdp_violations.iter().map(project_sdp));
        out.extend(
            c.metrics
                .iter()
                .filter(|m| m.warning && !m.suppressed)
                .map(project_threshold_breach),
        );
    }
    if let Some(s) = structural {
        s.warnings
            .iter()
            .filter(|w| w.dimension == Dimension::Coupling)
            .for_each(|w| out.push(project_structural(w)));
    }
    out
}

fn project_cycle(report: &CycleReport) -> CouplingFinding {
    CouplingFinding {
        common: Finding {
            file: String::new(),
            line: 0,
            column: 0,
            dimension: DIM,
            rule_id: "coupling/cycle".into(),
            message: format!("circular dependency: {}", report.modules.join(" -> ")),
            severity: SEV,
            suppressed: false,
        },
        kind: CouplingFindingKind::Cycle,
        details: CouplingFindingDetails::Cycle {
            modules: report.modules.clone(),
        },
    }
}

fn project_sdp(v: &SdpViolation) -> CouplingFinding {
    CouplingFinding {
        common: Finding {
            file: String::new(),
            line: 0,
            column: 0,
            dimension: DIM,
            rule_id: "coupling/sdp".into(),
            message: format!(
                "SDP violation: {} (instability {:.2}) depends on {} (instability {:.2})",
                v.from_module, v.from_instability, v.to_module, v.to_instability
            ),
            severity: SEV,
            suppressed: v.suppressed,
        },
        kind: CouplingFindingKind::SdpViolation,
        details: CouplingFindingDetails::SdpViolation {
            from_module: v.from_module.clone(),
            to_module: v.to_module.clone(),
            from_instability: v.from_instability,
            to_instability: v.to_instability,
        },
    }
}

fn project_threshold_breach(m: &CouplingMetrics) -> CouplingFinding {
    CouplingFinding {
        common: Finding {
            file: String::new(),
            line: 0,
            column: 0,
            dimension: DIM,
            rule_id: "coupling/threshold".into(),
            message: format!(
                "{}: instability {:.2} exceeds threshold (Ca={}, Ce={})",
                m.module_name, m.instability, m.afferent, m.efferent
            ),
            severity: SEV,
            suppressed: m.suppressed,
        },
        kind: CouplingFindingKind::ThresholdExceeded,
        details: CouplingFindingDetails::ThresholdExceeded {
            module_name: m.module_name.clone(),
            afferent: m.afferent,
            efferent: m.efferent,
            instability: m.instability,
        },
    }
}

fn project_structural(w: &StructuralWarning) -> CouplingFinding {
    let p = super::structural_shared::structural_pieces(w, DIM);
    CouplingFinding {
        common: p.common,
        kind: CouplingFindingKind::Structural,
        details: CouplingFindingDetails::Structural {
            item_name: w.name.clone(),
            code: p.code,
            detail: p.detail,
        },
    }
}
