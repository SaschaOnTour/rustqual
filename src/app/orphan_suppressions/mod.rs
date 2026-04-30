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

use std::collections::HashMap;

use crate::adapters::analyzers::iosp::Classification;
use crate::findings::Suppression;
use crate::report::OrphanSuppressionWarning;

// Window widths come from the shared `app::suppression_windows`
// module so the orphan detector and the `mark_*_suppressions`
// passes can't silently diverge.
use super::suppression_windows as windows;

/// How a finding position is matched against a suppression marker.
/// Mirrors the actual semantics of the per-dimension `mark_*`
/// functions so an orphan marker is only reported when no real
/// suppression site would accept it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatchMode {
    /// Line-proximity match: the finding's line must satisfy
    /// `sup.line <= line && line - sup.line <= n`.
    LineWindow(usize),
    /// File-global match: any marker anywhere in the file accepts.
    /// Used for SRP module warnings and Architecture findings whose
    /// marking logic is file-scoped.
    FileScope,
}

/// Detect `// qual:allow(...)` markers that do not match any finding
/// within their annotation window. Bare `// qual:allow` (no
/// dimensions) is a wildcard and matches any finding in range.
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
    analysis: &crate::report::AnalysisResult,
    config: &crate::config::Config,
) -> Vec<OrphanSuppressionWarning> {
    let positions = enumerate_finding_positions(analysis, config);
    let mut orphans: Vec<OrphanSuppressionWarning> = suppression_lines
        .iter()
        .flat_map(|(file, sups)| {
            sups.iter()
                .filter(|sup| is_verifiable(sup, file, &positions))
                .filter(|sup| !has_matching_finding(file, sup, &positions))
                .map(|sup| OrphanSuppressionWarning {
                    file: file.clone(),
                    line: sup.line,
                    dimensions: sup.dimensions.clone(),
                    reason: sup.reason.clone(),
                })
                .collect::<Vec<_>>()
        })
        .collect();
    orphans.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    orphans
}

/// True if the suppression can be verified against line-anchored
/// findings. Bare suppressions (empty `dimensions`) are wildcards
/// and always verifiable. Suppressions with at least one non-Coupling
/// dimension are verifiable on that dimension's positions. Coupling-
/// only suppressions are verifiable *only* when the file has a
/// line-anchored Coupling finding (e.g. a Structural OI/SIT/DEH/IET
/// warning carries `dimension == Coupling`). Pure module-global
/// coupling / cycle / SDP reports have no line anchor, so an
/// unverifiable coupling-only marker is skipped rather than reported
/// as a potentially-false orphan.
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
    let Some(file_positions) = positions.get(file) else {
        return false;
    };
    file_positions
        .iter()
        .any(|p| sup.covers(p.dim) && mode_accepts(sup.line, p.line, p.mode))
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

/// One finding's position for orphan matching.
#[derive(Debug, Clone, Copy)]
struct FindingPosition {
    line: usize,
    dim: crate::findings::Dimension,
    mode: MatchMode,
}

/// Enumerate every finding's position across all seven dimensions.
/// Findings with empty `file` (global coupling / SDP / cycle reports)
/// are skipped — they have no point-location a line-scoped
/// suppression could target. Coupling is handled at the is_verifiable
/// layer, not here.
/// Integration: delegates per-dimension collection to small helpers.
fn enumerate_finding_positions(
    analysis: &crate::report::AnalysisResult,
    config: &crate::config::Config,
) -> HashMap<String, Vec<FindingPosition>> {
    let mut out: HashMap<String, Vec<FindingPosition>> = HashMap::new();
    let mut push = |file: &str, line: usize, dim: crate::findings::Dimension, mode: MatchMode| {
        if !file.is_empty() {
            out.entry(file.to_string())
                .or_default()
                .push(FindingPosition { line, dim, mode });
        }
    };
    collect_iosp_complexity_positions(analysis, config, &mut push);
    collect_dry_positions(analysis, config, &mut push);
    collect_srp_positions(analysis, config, &mut push);
    collect_tq_positions(analysis, config, &mut push);
    collect_structural_positions(analysis, config, &mut push);
    collect_architecture_positions(analysis, config, &mut push);
    out
}

/// Positions for IOSP violations + Complexity warnings. Reads the raw
/// complexity metrics against config thresholds (not the
/// `*_warning` flags), so a suppressed `// qual:allow(complexity)`
/// marker — which clears those flags — still registers as a matching
/// target for the orphan checker. Mirrors the same config-gated
/// predicates that `apply_extended_warnings` uses (`detect_unsafe`,
/// `detect_error_handling`, `allow_expect`, `detect_magic_numbers`,
/// `is_test` skip for length / error-handling / magic numbers), so a
/// marker is only counted as non-orphan if the corresponding check is
/// actually enabled in the active config.
/// Operation: threshold checks pushing per-flag positions.
fn collect_iosp_complexity_positions<F>(
    analysis: &crate::report::AnalysisResult,
    config: &crate::config::Config,
    push: &mut F,
) where
    F: FnMut(&str, usize, crate::findings::Dimension, MatchMode),
{
    use crate::findings::Dimension;
    let mode = MatchMode::LineWindow(windows::DEFAULT);
    let complexity_enabled = config.complexity.enabled;
    analysis.results.iter().for_each(|f| {
        if matches!(f.classification, Classification::Violation { .. }) {
            push(&f.file, f.line, Dimension::Iosp, mode);
        }
        if !complexity_enabled {
            return;
        }
        if let Some(c) = &f.complexity {
            if complexity_predicates::would_trigger(f, c, &config.complexity) {
                push(&f.file, f.line, Dimension::Complexity, mode);
            }
            push_magic_numbers(f, c, &config.complexity, push);
        }
    });
}

/// Push complexity positions for every magic-number occurrence on the
/// function, honoring `detect_magic_numbers` and the test-function skip.
/// Operation: iteration + conditional push.
fn push_magic_numbers<F>(
    f: &crate::adapters::analyzers::iosp::FunctionAnalysis,
    c: &crate::adapters::analyzers::iosp::ComplexityMetrics,
    cx: &crate::config::sections::ComplexityConfig,
    push: &mut F,
) where
    F: FnMut(&str, usize, crate::findings::Dimension, MatchMode),
{
    if f.is_test || !cx.detect_magic_numbers {
        return;
    }
    let mode = MatchMode::LineWindow(windows::DEFAULT);
    c.magic_numbers.iter().for_each(|m| {
        push(
            &f.file,
            m.line,
            crate::findings::Dimension::Complexity,
            mode,
        )
    });
}

/// Positions for DRY findings (duplicates, dead code, fragments,
/// boilerplate, wildcards, repeated matches).
/// Operation: iterates DRY finding arrays pushing each entry.
fn collect_dry_positions<F>(
    analysis: &crate::report::AnalysisResult,
    config: &crate::config::Config,
    push: &mut F,
) where
    F: FnMut(&str, usize, crate::findings::Dimension, MatchMode),
{
    use crate::findings::Dimension;
    // DRY findings come from two top-level config toggles:
    // `duplicates.enabled` (DRY-001 duplicates, DRY-002 dead code,
    // DRY-003 fragments, DRY-004 wildcard imports, DRY-005 repeated
    // match patterns) and `boilerplate.enabled` (BP-001..BP-010
    // pattern family). If both are off, suppressing DRY is a no-op
    // and any qual:allow(dry) marker SHOULD surface as orphan.
    if !config.duplicates.enabled && !config.boilerplate.enabled {
        return;
    }
    // Default DRY window (duplicates, fragments, boilerplate,
    // repeated matches). Dead-code findings are intentionally *not*
    // included: they are not suppressible via `qual:allow(dry)` —
    // exclusions happen via `qual:api`, `qual:test_helper`,
    // `#[allow(dead_code)]`, or being a test function, all handled
    // at the declaration-collection layer. Including them here
    // would let an unrelated `qual:allow(dry)` marker falsely mask
    // a stale suppression as non-orphan.
    use crate::domain::findings::DryFindingKind;
    let mode = MatchMode::LineWindow(windows::DEFAULT);
    // Wildcards use a tighter window: `mark_wildcard_suppressions`
    // only accepts the marker on the same line or immediately above.
    let wildcard_mode = MatchMode::LineWindow(windows::WILDCARD);
    analysis.findings.dry.iter().for_each(|f| {
        let m = match f.kind {
            DryFindingKind::DuplicateExact
            | DryFindingKind::DuplicateSimilar
            | DryFindingKind::Fragment
            | DryFindingKind::Boilerplate
            | DryFindingKind::RepeatedMatch => mode,
            DryFindingKind::Wildcard => wildcard_mode,
            // Dead-code findings are intentionally *not* included: they are
            // not suppressible via `qual:allow(dry)` (see comment above).
            DryFindingKind::DeadCodeUncalled | DryFindingKind::DeadCodeTestOnly => return,
        };
        push(&f.common.file, f.common.line, Dimension::Dry, m);
    });
}

/// Positions for SRP struct/module/param warnings. Struct and param
/// warnings use the 5-line SRP suppression window; module warnings
/// are file-scoped because `mark_srp_suppressions` accepts any
/// `qual:allow(srp)` in the file as a module-level suppression.
/// Operation: iterates SRP warning arrays pushing each entry.
fn collect_srp_positions<F>(
    analysis: &crate::report::AnalysisResult,
    config: &crate::config::Config,
    push: &mut F,
) where
    F: FnMut(&str, usize, crate::findings::Dimension, MatchMode),
{
    use crate::domain::findings::SrpFindingKind;
    use crate::findings::Dimension;
    if !config.srp.enabled {
        return;
    }
    let line_mode = MatchMode::LineWindow(windows::SRP_STRUCT_PARAM);
    analysis.findings.srp.iter().for_each(|f| match f.kind {
        SrpFindingKind::StructCohesion | SrpFindingKind::ParameterCount => {
            push(&f.common.file, f.common.line, Dimension::Srp, line_mode);
        }
        SrpFindingKind::ModuleLength => {
            push(&f.common.file, 1, Dimension::Srp, MatchMode::FileScope);
        }
        // Structural findings are handled by collect_structural_positions.
        SrpFindingKind::Structural => {}
    });
}

/// Positions for Test-Quality warnings. TQ suppressions use a 5-line
/// window (mark_tq_suppressions).
/// Operation: iterates TQ warnings pushing each entry.
fn collect_tq_positions<F>(
    analysis: &crate::report::AnalysisResult,
    config: &crate::config::Config,
    push: &mut F,
) where
    F: FnMut(&str, usize, crate::findings::Dimension, MatchMode),
{
    use crate::findings::Dimension;
    if !config.test_quality.enabled {
        return;
    }
    let mode = MatchMode::LineWindow(windows::TQ);
    analysis.findings.test_quality.iter().for_each(|f| {
        push(&f.common.file, f.common.line, Dimension::TestQuality, mode);
    });
}

/// Positions for Structural binary-check warnings; each carries its
/// own mapped dimension (SRP or Coupling). Structural suppressions
/// use a 5-line window (mark_structural_suppressions).
/// Operation: iterates structural warnings pushing each entry.
fn collect_structural_positions<F>(
    analysis: &crate::report::AnalysisResult,
    config: &crate::config::Config,
    push: &mut F,
) where
    F: FnMut(&str, usize, crate::findings::Dimension, MatchMode),
{
    use crate::domain::findings::{CouplingFindingKind, SrpFindingKind};
    use crate::findings::Dimension;
    if !config.structural.enabled {
        return;
    }
    let mode = MatchMode::LineWindow(windows::STRUCTURAL);
    analysis
        .findings
        .srp
        .iter()
        .filter(|f| matches!(f.kind, SrpFindingKind::Structural))
        .for_each(|f| push(&f.common.file, f.common.line, Dimension::Srp, mode));
    analysis
        .findings
        .coupling
        .iter()
        .filter(|f| matches!(f.kind, CouplingFindingKind::Structural))
        .for_each(|f| push(&f.common.file, f.common.line, Dimension::Coupling, mode));
}

/// Positions for Architecture-dimension findings. Architecture
/// suppressions are file-scoped (mark_architecture_suppressions
/// accepts any `qual:allow(architecture)` anywhere in the file).
/// Operation: iterates architecture findings pushing each entry.
fn collect_architecture_positions<F>(
    analysis: &crate::report::AnalysisResult,
    config: &crate::config::Config,
    push: &mut F,
) where
    F: FnMut(&str, usize, crate::findings::Dimension, MatchMode),
{
    use crate::findings::Dimension;
    if !config.architecture.enabled {
        return;
    }
    analysis.findings.architecture.iter().for_each(|f| {
        push(
            &f.common.file,
            f.common.line,
            Dimension::Architecture,
            MatchMode::FileScope,
        )
    });
}
