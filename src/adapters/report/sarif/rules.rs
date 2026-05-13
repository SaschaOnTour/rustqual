use crate::domain::findings::{
    ComplexityFindingKind, CouplingFinding, CouplingFindingDetails, DryFinding, DryFindingDetails,
    DryFindingKind, SrpFinding, SrpFindingDetails, SrpFindingKind, TqFindingKind,
};

pub(super) fn complexity_rule(kind: ComplexityFindingKind) -> &'static str {
    match kind {
        ComplexityFindingKind::Cognitive => "CX-001",
        ComplexityFindingKind::Cyclomatic => "CX-002",
        ComplexityFindingKind::MagicNumber => "CX-003",
        ComplexityFindingKind::FunctionLength => "CX-004",
        ComplexityFindingKind::NestingDepth => "CX-005",
        ComplexityFindingKind::Unsafe => "CX-006",
        ComplexityFindingKind::ErrorHandling => "A20",
    }
}

pub(super) fn dry_rule(f: &DryFinding) -> &str {
    match (&f.kind, &f.details) {
        (DryFindingKind::DuplicateExact | DryFindingKind::DuplicateSimilar, _) => "DRY-001",
        (DryFindingKind::DeadCodeUncalled | DryFindingKind::DeadCodeTestOnly, _) => "DRY-002",
        (DryFindingKind::Fragment, _) => "DRY-003",
        (DryFindingKind::Wildcard, _) => "DRY-004",
        (DryFindingKind::RepeatedMatch, _) => "DRY-005",
        (DryFindingKind::Boilerplate, DryFindingDetails::Boilerplate { pattern_id, .. }) => {
            pattern_id
        }
        // Fallback (kind/details mismatch): use the same id as ordinary
        // boilerplate so the result still references a registered rule.
        (DryFindingKind::Boilerplate, _) => "BP-001",
    }
}

pub(super) fn srp_rule(f: &SrpFinding) -> &'static str {
    match (&f.kind, &f.details) {
        (SrpFindingKind::StructCohesion, _) => "SRP-001",
        (SrpFindingKind::ModuleLength, _) => "SRP-002",
        (SrpFindingKind::ParameterCount, _) => "SRP-003",
        (SrpFindingKind::Structural, SrpFindingDetails::Structural { code, .. }) => {
            structural_rule(code)
        }
        _ => "SRP-001",
    }
}

pub(super) fn coupling_rule(f: &CouplingFinding) -> &'static str {
    match &f.details {
        CouplingFindingDetails::Cycle { .. } => "CP-001",
        CouplingFindingDetails::SdpViolation { .. } => "CP-002",
        CouplingFindingDetails::ThresholdExceeded { .. } => "CP-003",
        CouplingFindingDetails::Structural { code, .. } => structural_rule(code),
    }
}

pub(super) fn tq_rule(kind: &TqFindingKind) -> &'static str {
    kind.meta().sarif_rule
}

fn structural_rule(code: &str) -> &'static str {
    match code {
        "BTC" => "BTC",
        "SLM" => "SLM",
        "NMS" => "NMS",
        "OI" => "OI",
        "SIT" => "SIT",
        "DEH" => "DEH",
        "IET" => "IET",
        _ => "BTC",
    }
}

/// Build the SARIF rules array with all known rule definitions.
/// Operation: static data construction, no own calls.
pub(super) fn sarif_rules() -> Vec<serde_json::Value> {
    vec![
        rule(
            "iosp/violation",
            "Function violates the Integration Operation Segregation Principle",
        ),
        rule("CX-001", "Cognitive complexity exceeds threshold"),
        rule("CX-002", "Cyclomatic complexity exceeds threshold"),
        rule("CX-003", "Magic number literal in non-const context"),
        rule("CP-001", "Circular module dependency"),
        rule("DRY-001", "Duplicate function detected"),
        rule("DRY-002", "Dead code detected"),
        rule("DRY-003", "Duplicate code fragment"),
        rule(
            "SRP-001",
            "Struct may violate Single Responsibility Principle",
        ),
        rule("SRP-002", "Module has excessive production code length"),
        rule(
            "SRP-003",
            "Function has too many parameters — reduce parameter count",
        ),
        rule("CX-004", "Function length exceeds threshold"),
        rule("CX-005", "Nesting depth exceeds threshold"),
        rule("CX-006", "Unsafe block detected"),
        rule("A20", "Error handling issue (unwrap/expect/panic/todo)"),
        rule("DRY-004", "Wildcard import (use module::*)"),
        rule("CP-002", "Stable Dependencies Principle violation"),
        rule("CP-003", "Module instability exceeds configured threshold"),
        rule("TQ-001", "Test function has no assertions"),
        rule(
            "TQ-002",
            "Test function does not call any production function",
        ),
        rule("TQ-003", "Production function is untested"),
        rule("TQ-004", "Production function has no coverage"),
        rule("TQ-005", "Untested logic branches (uncovered lines)"),
        rule("BTC", "Broken trait contract: all methods are stubs"),
        rule("SLM", "Selfless method: takes self but never references it"),
        rule(
            "NMS",
            "Needless &mut self: takes &mut self but never mutates",
        ),
        rule(
            "OI",
            "Orphaned impl: impl block in different file than type",
        ),
        rule(
            "SIT",
            "Single-impl trait: non-pub trait with exactly one implementation",
        ),
        rule("DEH", "Downcast escape hatch: use of Any::downcast"),
        rule("IET", "Inconsistent error types in module"),
        rule("DRY-005", "Repeated match pattern across functions"),
        rule("BP-001", "Trivial From implementation (derivable)"),
        rule("BP-002", "Trivial Display implementation (derivable)"),
        rule(
            "BP-003",
            "Trivial getter/setter (consider field visibility)",
        ),
        rule("BP-004", "Builder pattern (consider derive macro)"),
        rule("BP-005", "Manual Default implementation (derivable)"),
        rule("BP-006", "Repetitive match mapping"),
        rule("BP-007", "Error enum boilerplate (consider thiserror)"),
        rule("BP-008", "Clone-heavy conversion"),
        rule("BP-009", "Struct update boilerplate"),
        rule("BP-010", "Format string repetition"),
        rule("SUP-001", "Suppression ratio exceeds configured maximum"),
        rule(
            "ORPHAN-001",
            "Stale qual:allow marker: no finding in the annotation window",
        ),
        // Architecture dimension — hierarchical rule IDs.
        // Pattern + trait_contract have dynamic sub-kinds (the user-defined
        // rule's name / check string); only the base IDs registered here.
        rule("architecture/layer", "Layer rule violation"),
        rule(
            "architecture/layer/unmatched",
            "File doesn't match any configured layer",
        ),
        rule(
            "architecture/forbidden",
            "Forbidden-edge violation between layers",
        ),
        rule(
            "architecture/pattern",
            "Symbol-pattern violation (path/method/macro)",
        ),
        rule("architecture/trait_contract", "Trait-contract violation"),
        rule(
            "architecture/call_parity/no_delegation",
            "Adapter pub fn does not reach the target layer",
        ),
        rule(
            "architecture/call_parity/missing_adapter",
            "Target pub fn is not reached by every adapter (or is orphaned)",
        ),
        rule(
            "architecture/call_parity/multi_touchpoint",
            "Adapter pub fn has multiple target touchpoints",
        ),
        rule(
            "architecture/call_parity/multiplicity_mismatch",
            "Target pub fn reached with divergent handler counts across adapters",
        ),
    ]
}

/// Build a single SARIF rule entry.
/// Operation: JSON construction.
fn rule(id: &str, description: &str) -> serde_json::Value {
    let mut entry = serde_json::json!({
        "id": id,
        "shortDescription": { "text": description }
    });
    if id == "iosp/violation" {
        if let Some(obj) = entry.as_object_mut() {
            obj.insert(
                "helpUri".to_string(),
                serde_json::json!("https://flow-design.info/"),
            );
        }
    }
    entry
}
