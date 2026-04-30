//! Shared helpers for projecting `StructuralWarning` into typed
//! Findings. The structural binary checks (BTC/SLM/NMS for SRP,
//! OI/SIT/DEH/IET for Coupling) share most projection logic — only
//! the destination Finding type differs. This module builds the
//! common pieces (rule_id, message, code, detail) once.

use crate::adapters::analyzers::structural::StructuralWarning;
use crate::domain::{Dimension, Finding, Severity};

/// Common pieces extracted from a `StructuralWarning` that both the
/// SRP and Coupling projection paths need: a typed `Finding` ready
/// to embed plus the `code`/`detail` strings used in the dimension-
/// specific `details` enum variant.
pub(super) struct StructuralPieces {
    pub common: Finding,
    pub code: String,
    pub detail: String,
}

/// Build a typed `Finding` (rule_id, message, file/line, severity,
/// suppression) plus the `code`/`detail` strings for a structural
/// warning. Both SRP and Coupling projection paths call this.
pub(super) fn structural_pieces(w: &StructuralWarning, dim: Dimension) -> StructuralPieces {
    let code = w.kind.code().to_string();
    let detail = w.kind.detail().to_string();
    let dim_prefix = match dim {
        Dimension::Srp => "srp",
        Dimension::Coupling => "coupling",
        _ => "structural",
    };
    StructuralPieces {
        common: Finding {
            file: w.file.clone(),
            line: w.line,
            column: 0,
            dimension: dim,
            rule_id: format!("{}/structural/{}", dim_prefix, code.to_lowercase()),
            message: format!("{}: '{}' \u{2014} {}", code, w.name, detail),
            severity: Severity::Medium,
            suppressed: w.suppressed,
        },
        code,
        detail,
    }
}
