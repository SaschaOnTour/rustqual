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
        (DryFindingKind::DeadTypeUnused | DryFindingKind::DeadTypeTestOnly, _) => "DRY-006",
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

/// Build the SARIF rules array from the rule-card registry — the single
/// source of truth for catalog ids and titles (pattern + trait_contract
/// dynamic sub-kinds are added at emit time, not registered here).
/// Operation: registry map, no own calls outside the registry read.
pub(super) fn sarif_rules() -> Vec<serde_json::Value> {
    crate::domain::rule_cards::all_rule_cards()
        .map(|c| rule(c.id, c.title))
        .collect()
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
