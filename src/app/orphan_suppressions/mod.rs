//! Orphan-suppression detector.
//!
//! An orphan `// qual:allow(...)` marker is one that doesn't match any
//! finding within its annotation window — typically a stale
//! suppression (the underlying finding was fixed or moved) or a
//! misplaced annotation. Orphans are emitted as a distinct finding
//! category (`ORPHAN_SUPPRESSION`) so they show up in every output
//! format (text, JSON, AI, SARIF, ...) just like any other finding —
//! one-shot `--format ai` invocations don't miss them.

mod complexity_predicates;
mod positions;

use std::collections::HashMap;

use crate::adapters::suppression::qual_allow::InvalidQualAllow;
use crate::domain::findings::{OrphanKind, OrphanSuppression};
use crate::findings::Suppression;

use positions::{enumerate_finding_positions, FindingPosition, MatchMode};

/// Detect `// qual:allow(...)` markers that do not match any finding
/// within their annotation window. Two input streams:
///
/// - `suppression_lines`: real `Suppression` entries from the parser.
///   Always carry at least one recognised dimension since 1.2.3 (bare
///   and empty-dim forms are silently ignored upstream).
/// - `invalid_qual_allow_lines`: side-channel for malformed markers
///   (typos like `qual:allow(srp_params)`, unclosed parens). Always
///   surface as orphan — they suppress nothing.
///
/// Coupling-only markers are handled specially: they are verifiable
/// when the file has at least one line-anchored Coupling finding
/// (e.g. a Structural OI/SIT/DEH/IET warning carries `dimension ==
/// Coupling`). If the file has no line-anchored Coupling position —
/// only pure module-global coupling / cycle / SDP reports — the
/// marker is skipped (not reported as orphan), because we cannot
/// verify line-scoped match against a module-scoped finding.
/// Integration: collects finding positions, then filters unmatched markers.
pub(crate) fn detect_orphan_suppressions(
    suppression_lines: &HashMap<String, Vec<Suppression>>,
    invalid_qual_allow_lines: &HashMap<String, Vec<(usize, InvalidQualAllow)>>,
    analysis: &crate::report::AnalysisResult,
    config: &crate::config::Config,
) -> Vec<OrphanSuppression> {
    let positions = enumerate_finding_positions(analysis, config);
    let headroom = config.suppression.pin_headroom;
    let mut orphans: Vec<OrphanSuppression> = suppression_lines
        .iter()
        .flat_map(|(file, sups)| {
            sups.iter()
                .filter_map(|sup| classify_marker(file, sup, &positions, headroom))
                .collect::<Vec<_>>()
        })
        .collect();
    orphans.extend(invalid_marker_orphans(invalid_qual_allow_lines));
    orphans.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    orphans
}

/// Classify a single marker into an orphan finding, or `None` when it is a
/// healthy suppression. A marker is reported when it is verifiable and
/// either (a) matches no finding of its (dimension, target) — *stale*, or
/// (b) is a metric pin sitting more than `headroom` above the value it
/// covers — *too-loose*. A too-tight pin (its finding re-fires above it) is
/// healthy, not orphan.
/// Operation: verifiability + match/too-loose dispatch reaching helpers.
fn classify_marker(
    file: &str,
    sup: &Suppression,
    positions: &HashMap<String, Vec<FindingPosition>>,
    headroom: f64,
) -> Option<OrphanSuppression> {
    if !is_verifiable(sup, file, positions) {
        return None;
    }
    if !has_matching_finding(file, sup, positions) {
        return Some(orphan(file, sup, OrphanKind::Stale, sup.reason.clone()));
    }
    let value = too_loose_value(file, sup, positions, headroom)?;
    let pin = sup.target.as_ref().and_then(|t| t.pin).unwrap_or_default();
    let reason = format!(
        "pin {} sits >{:.0}% above the actual value {} — tighten to ~{} or remove",
        fmt_num(pin),
        headroom * 100.0,
        fmt_num(value),
        fmt_num(value),
    );
    Some(orphan(file, sup, OrphanKind::PinTooLoose, Some(reason)))
}

/// Build an `OrphanSuppression` for `sup` at its marker line.
/// Operation: struct construction, no own calls.
fn orphan(
    file: &str,
    sup: &Suppression,
    kind: OrphanKind,
    reason: Option<String>,
) -> OrphanSuppression {
    OrphanSuppression {
        file: file.to_string(),
        line: sup.line,
        dimensions: sup.dimensions.clone(),
        reason,
        kind,
    }
}

/// Render a metric value without a trailing `.0` for whole numbers (metric
/// thresholds are integers; only `max_instability` is fractional).
/// Operation: float formatting branch, no own calls.
fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// For a metric pin that matches a finding, the largest covered value that
/// the pin sits too far above (`pin > value × (1 + headroom)`), or `None`
/// when the pin is healthy (within headroom, or too tight so nothing is
/// covered). The *largest* covered value is used: a pin must be tight to
/// whatever it actually silences, and a finding above the pin re-fires and
/// does not constrain looseness.
/// Operation: filter/fold over matching positions, no own calls.
fn too_loose_value(
    file: &str,
    sup: &Suppression,
    positions: &HashMap<String, Vec<FindingPosition>>,
    headroom: f64,
) -> Option<f64> {
    let pin = sup.target.as_ref()?.pin?;
    let covered_max = positions
        .get(file)?
        .iter()
        .filter(|p| position_matches(sup, p))
        .filter_map(|p| p.value)
        .filter(|&v| v <= pin)
        .fold(None, |acc: Option<f64>, v| {
            Some(acc.map_or(v, |a| a.max(v)))
        })?;
    (pin > covered_max * (1.0 + headroom)).then_some(covered_max)
}

/// Project the `// qual:allow(<unknown>)` side-channel into orphan
/// findings. These markers don't suppress anything (they're stored
/// in a separate map, never reach `Suppression::covers`), but the
/// author should still see a stale-marker finding so the typo is
/// visible. Operation: per-marker projection.
fn invalid_marker_orphans(
    invalid_qual_allow_lines: &HashMap<String, Vec<(usize, InvalidQualAllow)>>,
) -> Vec<OrphanSuppression> {
    invalid_qual_allow_lines
        .iter()
        .flat_map(|(file, markers)| {
            markers.iter().map(move |(line, kind)| OrphanSuppression {
                file: file.clone(),
                line: *line,
                dimensions: Vec::new(),
                reason: Some(kind.reason()),
                kind: OrphanKind::Stale,
            })
        })
        .collect()
}

/// True if the suppression can be verified against line-anchored
/// findings. Suppressions with at least one non-Coupling dimension
/// are verifiable on that dimension's positions. Coupling-only
/// suppressions are verifiable *only* when the file has a
/// line-anchored Coupling finding (e.g. a Structural OI/SIT/DEH/IET
/// warning carries `dimension == Coupling`). Pure module-global
/// coupling / cycle / SDP reports have no line anchor, so an
/// unverifiable coupling-only marker is skipped rather than reported
/// as a potentially-false orphan.
///
/// **Defensive-only:** the empty-dimensions branch (treat as
/// wildcard) is unreachable from production since 1.2.3 — the
/// parser drops bare/empty `qual:allow` upstream. The branch stays
/// to keep test fixtures and any future internal misuse stable
/// rather than producing confusing false-orphan reports.
/// Operation: predicate over dimensions + file position lookup.
fn is_verifiable(
    sup: &Suppression,
    file: &str,
    positions: &HashMap<String, Vec<FindingPosition>>,
) -> bool {
    use crate::findings::Dimension;
    if sup.dimensions.is_empty() {
        return true;
    }
    if sup.dimensions.iter().any(|d| *d != Dimension::Coupling) {
        return true;
    }
    // Coupling-only marker: verifiable iff the file has a line-anchored
    // Coupling finding.
    positions
        .get(file)
        .is_some_and(|ps| ps.iter().any(|p| p.dim == Dimension::Coupling))
}

/// True if some finding in `file` matches the suppression under its
/// dimension-specific match mode (line window of the right width, or
/// file-global scope).
/// Operation: hashmap lookup + predicate logic, no own calls.
fn has_matching_finding(
    file: &str,
    sup: &Suppression,
    positions: &HashMap<String, Vec<FindingPosition>>,
) -> bool {
    positions
        .get(file)
        .is_some_and(|ps| ps.iter().any(|p| position_matches(sup, p)))
}

/// True if a finding position is what `sup` is allowed to silence: same
/// dimension and within the match window, and — for a *targeted* marker —
/// the same finding-kind (`target`). A blanket marker matches any covered
/// finding-kind of the dimension; a targeted marker matches only its own
/// kind, so a `file_length` pin no longer counts a god-struct finding as a
/// match. The pin *value* is intentionally not consulted here: presence of
/// the kind makes the marker non-stale; whether the pin is too loose/tight
/// is decided separately by `too_loose_value`.
/// Operation: dimension + target + window predicate, no own calls.
fn position_matches(sup: &Suppression, p: &FindingPosition) -> bool {
    if !sup.covers(p.dim) || !mode_accepts(sup.line, p.line, p.mode) {
        return false;
    }
    match &sup.target {
        None => true,
        Some(t) => p.target == Some(t.name.as_str()),
    }
}

/// True if a suppression at `sup_line` accepts a finding at
/// `finding_line` under the given match mode.
/// Operation: match on mode + comparison.
fn mode_accepts(sup_line: usize, finding_line: usize, mode: MatchMode) -> bool {
    match mode {
        MatchMode::FileScope => true,
        MatchMode::LineWindow(n) => finding_line >= sup_line && finding_line - sup_line <= n,
    }
}
