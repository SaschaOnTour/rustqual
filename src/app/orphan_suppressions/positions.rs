//! Finding-position enumeration for the orphan detector.
//!
//! Walks the seven dimensions' findings (and the raw complexity metrics) and
//! emits one [`FindingPosition`] per suppressible finding-kind, tagged with
//! its suppression target and — for pinnable metrics — its value. The
//! decision layer in the parent module matches markers against these
//! positions and judges pin headroom; this module only *enumerates*.

use std::collections::HashMap;

use crate::adapters::analyzers::iosp::Classification;
use crate::app::suppression_windows as windows;

use super::complexity_predicates;

/// How a finding position is matched against a suppression marker.
/// Mirrors the actual semantics of the per-dimension `mark_*`
/// functions so an orphan marker is only reported when no real
/// suppression site would accept it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MatchMode {
    /// Line-proximity match: the finding's line must satisfy
    /// `sup.line <= line && line - sup.line <= n`.
    LineWindow(usize),
    /// File-global match: any marker anywhere in the file accepts.
    /// Used for SRP module warnings (line 1, file-level marker) — the
    /// remaining dimensions, including Architecture, use line-window
    /// matching that mirrors their `mark_*_suppressions` semantics.
    FileScope,
}

/// One finding's position for orphan matching.
#[derive(Debug, Clone, Copy)]
pub(super) struct FindingPosition {
    pub(super) line: usize,
    pub(super) dim: crate::findings::Dimension,
    pub(super) mode: MatchMode,
    /// The suppression-target name this finding is silenced by (e.g.
    /// `"max_cognitive"`, `"file_length"`, `"god_struct"`, the structural
    /// codes `"oi"`/`"sit"`/…), so a targeted marker only matches findings of
    /// its own kind. `None` for findings with no suppressible target (e.g.
    /// IOSP violations) — those are matched only by a blanket marker.
    pub(super) target: Option<&'static str>,
    /// The metric value for a pinnable target (cognitive count, line count,
    /// parameter count, …), used by the too-loose-pin check. `None` for
    /// boolean targets and untargeted positions.
    pub(super) value: Option<f64>,
}

/// Enumerate every finding's position across all seven dimensions.
/// Findings with empty `file` (global coupling / SDP / cycle reports)
/// are skipped — they have no point-location a line-scoped
/// suppression could target. Coupling is handled at the is_verifiable
/// layer, not here.
/// Integration: delegates per-dimension collection to small helpers.
pub(super) fn enumerate_finding_positions(
    analysis: &crate::report::AnalysisResult,
    config: &crate::config::Config,
) -> HashMap<String, Vec<FindingPosition>> {
    let mut out: HashMap<String, Vec<FindingPosition>> = HashMap::new();
    let mut push = |file: &str, pos: FindingPosition| {
        if !file.is_empty() {
            out.entry(file.to_string()).or_default().push(pos);
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
    F: FnMut(&str, FindingPosition),
{
    use crate::findings::Dimension;
    let mode = MatchMode::LineWindow(windows::DEFAULT);
    let mk = |line, dim, target, value| FindingPosition {
        line,
        dim,
        mode,
        target,
        value,
    };
    let complexity_enabled = config.complexity.enabled;
    let test_max_lines = config
        .tests
        .max_function_lines
        .unwrap_or(config.complexity.max_function_lines);
    analysis.results.iter().for_each(|f| {
        if matches!(f.classification, Classification::Violation { .. }) {
            push(&f.file, mk(f.line, Dimension::Iosp, None, None));
        }
        if !complexity_enabled {
            return;
        }
        if let Some(c) = &f.complexity {
            for (target, value) in
                complexity_predicates::triggered_targets(f, c, &config.complexity, test_max_lines)
            {
                push(
                    &f.file,
                    mk(f.line, Dimension::Complexity, Some(target), value),
                );
            }
            // One `magic_numbers` position per function (a boolean target),
            // anchored at the *function* line — a literal deep in the body
            // would otherwise fall outside a marker's window and make a valid
            // suppression look stale. Honors `detect_magic_numbers` + is_test.
            let cx = &config.complexity;
            if cx.detect_magic_numbers && !f.is_test && !c.magic_numbers.is_empty() {
                push(
                    &f.file,
                    mk(f.line, Dimension::Complexity, Some("magic_numbers"), None),
                );
            }
        }
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
    F: FnMut(&str, FindingPosition),
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
        let (m, target) = match f.kind {
            DryFindingKind::DuplicateExact | DryFindingKind::DuplicateSimilar => {
                (mode, "duplicate")
            }
            DryFindingKind::Fragment => (mode, "fragment"),
            DryFindingKind::Boilerplate => (mode, "boilerplate"),
            DryFindingKind::RepeatedMatch => (mode, "repeated_matches"),
            DryFindingKind::Wildcard => (wildcard_mode, "wildcard_imports"),
            // Dead-code findings are intentionally *not* included: they are
            // not suppressible via `qual:allow(dry)` (see comment above).
            DryFindingKind::DeadCodeUncalled
            | DryFindingKind::DeadCodeTestOnly
            | DryFindingKind::DeadTypeUnused
            | DryFindingKind::DeadTypeTestOnly => return,
        };
        push(
            &f.common.file,
            FindingPosition {
                line: f.common.line,
                dim: Dimension::Dry,
                mode: m,
                target: Some(target),
                value: None,
            },
        );
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
    F: FnMut(&str, FindingPosition),
{
    use crate::findings::Dimension;
    if !config.srp.enabled {
        return;
    }
    let line_mode = MatchMode::LineWindow(windows::SRP_STRUCT_PARAM);
    let max_clusters = config.srp.max_independent_clusters;
    analysis.findings.srp.iter().for_each(|f| {
        for (line, mode, target, value) in srp_finding_targets(f, line_mode, max_clusters) {
            push(
                &f.common.file,
                FindingPosition {
                    line,
                    dim: Dimension::Srp,
                    mode,
                    target: Some(target),
                    value,
                },
            );
        }
    });
}

/// The `(line, mode, target, value)` positions a single SRP finding
/// contributes: `god_struct` (cohesion), `max_parameters` (params), or the
/// module-length targets `file_length` / `max_independent_clusters` — the
/// latter **only for the component that actually fired** (`length_score >
/// 1.0` and/or `clusters > max_clusters`), mirroring
/// `srp_suppressions::module_warning_suppressed`; a position for an inactive
/// component would let a pin that silences nothing escape stale detection or
/// mis-fire as too-loose. Structural findings are handled elsewhere.
/// Operation: kind/details dispatch producing position tuples.
fn srp_finding_targets(
    f: &crate::domain::findings::SrpFinding,
    line_mode: MatchMode,
    max_clusters: usize,
) -> Vec<(usize, MatchMode, &'static str, Option<f64>)> {
    use crate::domain::findings::{SrpFindingDetails, SrpFindingKind};
    let scope = MatchMode::FileScope;
    match (&f.kind, &f.details) {
        (SrpFindingKind::StructCohesion, _) => {
            vec![(f.common.line, line_mode, "god_struct", None)]
        }
        (
            SrpFindingKind::ParameterCount,
            SrpFindingDetails::ParameterCount {
                parameter_count, ..
            },
        ) => vec![(
            f.common.line,
            line_mode,
            "max_parameters",
            Some(*parameter_count as f64),
        )],
        (
            SrpFindingKind::ModuleLength,
            SrpFindingDetails::ModuleLength {
                production_lines,
                independent_clusters,
                length_score,
                ..
            },
        ) => [
            (*length_score > 1.0).then_some((
                1,
                scope,
                "file_length",
                Some(*production_lines as f64),
            )),
            (*independent_clusters > max_clusters).then_some((
                1,
                scope,
                "max_independent_clusters",
                Some(*independent_clusters as f64),
            )),
        ]
        .into_iter()
        .flatten()
        .collect(),
        // Structural BTC/SLM/NMS findings are handled by
        // `collect_structural_positions` and contribute nothing here.
        (SrpFindingKind::Structural, _) => vec![],
        // kind and details are paired atomically by the projection layer; a
        // mismatch is an internal bug, so fail loudly in debug rather than
        // silently dropping the position (which would become a false orphan).
        (kind, _) => {
            debug_assert!(false, "SRP kind/details mismatch for {kind:?}");
            vec![]
        }
    }
}

/// Positions for Test-Quality warnings. TQ suppressions use a 5-line
/// window (mark_tq_suppressions).
/// Operation: iterates TQ warnings pushing each entry.
fn collect_tq_positions<F>(
    analysis: &crate::report::AnalysisResult,
    config: &crate::config::Config,
    push: &mut F,
) where
    F: FnMut(&str, FindingPosition),
{
    use crate::findings::Dimension;
    if !config.test_quality.enabled {
        return;
    }
    let mode = MatchMode::LineWindow(windows::TQ);
    analysis.findings.test_quality.iter().for_each(|f| {
        // `json_kind` (no_assertion/no_sut/untested/uncovered/untested_logic)
        // is exactly the TQ target vocabulary, so it doubles as the target.
        push(
            &f.common.file,
            FindingPosition {
                line: f.common.line,
                dim: Dimension::TestQuality,
                mode,
                target: Some(f.kind.meta().json_kind),
                value: None,
            },
        );
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
    F: FnMut(&str, FindingPosition),
{
    use crate::adapters::analyzers::structural::target_name_for_code;
    use crate::domain::findings::{
        CouplingFindingDetails, CouplingFindingKind, SrpFindingDetails, SrpFindingKind,
    };
    use crate::findings::Dimension;
    if !config.structural.enabled {
        return;
    }
    let mode = MatchMode::LineWindow(windows::STRUCTURAL);
    // Each structural binary check (BTC/SLM/NMS on the SRP side, OI/SIT/DEH/IET
    // on the coupling side) is tagged with its own boolean target (the
    // lowercased code), so a targeted `allow(coupling, oi)` matches its own
    // finding and a stale one surfaces as an orphan.
    let pos = |line, dim, code: &str| FindingPosition {
        line,
        dim,
        mode,
        target: target_name_for_code(code),
        value: None,
    };
    analysis.findings.srp.iter().for_each(|f| {
        if let (SrpFindingKind::Structural, SrpFindingDetails::Structural { code, .. }) =
            (&f.kind, &f.details)
        {
            push(&f.common.file, pos(f.common.line, Dimension::Srp, code));
        }
    });
    analysis.findings.coupling.iter().for_each(|f| {
        if let (CouplingFindingKind::Structural, CouplingFindingDetails::Structural { code, .. }) =
            (&f.kind, &f.details)
        {
            push(
                &f.common.file,
                pos(f.common.line, Dimension::Coupling, code),
            );
        }
    });
}

/// Positions for Architecture-dimension findings. Architecture
/// suppressions are window-scoped (mark_architecture_suppressions
/// accepts a `qual:allow(architecture)` only within the marker's
/// annotation window above the finding) — orphan detection mirrors
/// that semantic so a marker for one helper is reported as stale
/// when the only architecture finding in the file lives elsewhere.
/// Operation: iterates architecture findings pushing each entry.
fn collect_architecture_positions<F>(
    analysis: &crate::report::AnalysisResult,
    config: &crate::config::Config,
    push: &mut F,
) where
    F: FnMut(&str, FindingPosition),
{
    use crate::findings::Dimension;
    if !config.architecture.enabled {
        return;
    }
    let mode = MatchMode::LineWindow(windows::DEFAULT);
    analysis.findings.architecture.iter().for_each(|f| {
        // Resolve the finding's `architecture/<family>/…` segment to the
        // canonical (`'static`) target name, so a targeted `allow(
        // architecture, layer)` matches only layer findings. A family with
        // no vocabulary entry leaves `target = None` (blanket-only).
        let family = crate::app::architecture::arch_family(&f.common.rule_id);
        let target = crate::domain::target_names(Dimension::Architecture)
            .iter()
            .copied()
            .find(|n| *n == family);
        push(
            &f.common.file,
            FindingPosition {
                line: f.common.line,
                dim: Dimension::Architecture,
                mode,
                target,
                value: None,
            },
        );
    });
}
