//! Orphan-suppression detector.
//!
//! An orphan `// qual:allow(...)` marker is one that doesn't match any
//! finding within its annotation window — typically a stale
//! suppression (the underlying finding was fixed or moved) or a
//! misplaced annotation. Orphans are emitted as a distinct finding
//! category (`ORPHAN_SUPPRESSION`) so they show up in every output
//! format (text, JSON, AI, SARIF, ...) just like any other finding —
//! one-shot `--format ai` invocations don't miss them.

use std::collections::HashMap;

use crate::adapters::analyzers::iosp::Classification;
use crate::findings::Suppression;
use crate::report::OrphanSuppressionWarning;

/// Detect `// qual:allow(...)` markers that do not match any finding
/// within their annotation window. Bare `// qual:allow` (no
/// dimensions) is a wildcard and matches any finding in range.
/// Markers that only suppress Coupling are skipped because coupling
/// warnings are module-global and have no point location a
/// line-scoped check could verify.
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
                .filter(|sup| is_verifiable(sup))
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

/// True if at least one of the suppression's dimensions has a
/// point-location the orphan checker can verify. Bare suppressions
/// (empty `dimensions`) are wildcards and verifiable. Coupling-only
/// suppressions are *not* verifiable — Coupling warnings are emitted
/// at the module level with no file/line anchor.
/// Operation: predicate over the dimension list.
fn is_verifiable(sup: &Suppression) -> bool {
    use crate::findings::Dimension;
    if sup.dimensions.is_empty() {
        return true;
    }
    sup.dimensions.iter().any(|d| *d != Dimension::Coupling)
}

/// True if some finding in `file` is within the suppression's
/// annotation window and covered by its dimension set.
/// Operation: hashmap lookup + predicate logic, no own calls.
fn has_matching_finding(
    file: &str,
    sup: &Suppression,
    positions: &HashMap<String, Vec<(usize, crate::findings::Dimension)>>,
) -> bool {
    let Some(file_positions) = positions.get(file) else {
        return false;
    };
    file_positions.iter().any(|(line, dim)| {
        let in_window = *line >= sup.line && *line - sup.line <= crate::findings::ANNOTATION_WINDOW;
        in_window && sup.covers(*dim)
    })
}

/// Enumerate every finding's (file, line, dimension) across all seven
/// dimensions. Findings with empty `file` (global coupling / SDP /
/// cycle reports) are skipped — they have no point-location a
/// line-scoped suppression could target.
/// Integration: delegates per-dimension collection to small helpers.
fn enumerate_finding_positions(
    analysis: &crate::report::AnalysisResult,
    config: &crate::config::Config,
) -> HashMap<String, Vec<(usize, crate::findings::Dimension)>> {
    let mut out: HashMap<String, Vec<(usize, crate::findings::Dimension)>> = HashMap::new();
    let mut push = |file: &str, line: usize, dim: crate::findings::Dimension| {
        if !file.is_empty() {
            out.entry(file.to_string()).or_default().push((line, dim));
        }
    };
    collect_iosp_complexity_positions(analysis, config, &mut push);
    collect_dry_positions(analysis, &mut push);
    collect_srp_positions(analysis, &mut push);
    collect_tq_positions(analysis, &mut push);
    collect_structural_positions(analysis, &mut push);
    collect_architecture_positions(analysis, &mut push);
    out
}

/// Positions for IOSP violations + Complexity warnings. Reads the raw
/// complexity metrics against config thresholds (not the
/// `*_warning` flags), so a suppressed `// qual:allow(complexity)`
/// marker — which clears those flags — still registers as a matching
/// target for the orphan checker. Same for IOSP: we check whether the
/// function `has_logic + has_own_calls` shape would be a Violation even
/// if it was reclassified away by suppression.
/// Operation: threshold checks pushing per-flag positions.
fn collect_iosp_complexity_positions<F>(
    analysis: &crate::report::AnalysisResult,
    config: &crate::config::Config,
    push: &mut F,
) where
    F: FnMut(&str, usize, crate::findings::Dimension),
{
    use crate::findings::Dimension;
    analysis.results.iter().for_each(|f| {
        if matches!(f.classification, Classification::Violation { .. }) {
            push(&f.file, f.line, Dimension::Iosp);
        }
        if let Some(c) = &f.complexity {
            if c.cognitive_complexity > config.complexity.max_cognitive
                || c.cyclomatic_complexity > config.complexity.max_cyclomatic
                || c.max_nesting > config.complexity.max_nesting_depth
                || c.function_lines > config.complexity.max_function_lines
                || c.unsafe_blocks > 0
                || c.unwrap_count > 0
                || c.expect_count > 0
                || c.panic_count > 0
                || c.todo_count > 0
            {
                push(&f.file, f.line, Dimension::Complexity);
            }
            if !f.is_test {
                c.magic_numbers
                    .iter()
                    .for_each(|m| push(&f.file, m.line, Dimension::Complexity));
            }
        }
    });
}

/// Positions for DRY findings (duplicates, dead code, fragments,
/// boilerplate, wildcards, repeated matches).
/// Operation: iterates DRY finding arrays pushing each entry.
fn collect_dry_positions<F>(analysis: &crate::report::AnalysisResult, push: &mut F)
where
    F: FnMut(&str, usize, crate::findings::Dimension),
{
    use crate::findings::Dimension;
    analysis.duplicates.iter().for_each(|g| {
        g.entries
            .iter()
            .for_each(|e| push(&e.file, e.line, Dimension::Dry));
    });
    analysis
        .dead_code
        .iter()
        .for_each(|w| push(&w.file, w.line, Dimension::Dry));
    analysis.fragments.iter().for_each(|g| {
        g.entries
            .iter()
            .for_each(|e| push(&e.file, e.start_line, Dimension::Dry));
    });
    analysis
        .boilerplate
        .iter()
        .for_each(|b| push(&b.file, b.line, Dimension::Dry));
    analysis
        .wildcard_warnings
        .iter()
        .for_each(|w| push(&w.file, w.line, Dimension::Dry));
    analysis.repeated_matches.iter().for_each(|g| {
        g.entries
            .iter()
            .for_each(|e| push(&e.file, e.line, Dimension::Dry));
    });
}

/// Positions for SRP struct/module/param warnings.
/// Operation: iterates SRP warning arrays pushing each entry.
fn collect_srp_positions<F>(analysis: &crate::report::AnalysisResult, push: &mut F)
where
    F: FnMut(&str, usize, crate::findings::Dimension),
{
    use crate::findings::Dimension;
    let Some(srp) = &analysis.srp else { return };
    srp.struct_warnings
        .iter()
        .for_each(|w| push(&w.file, w.line, Dimension::Srp));
    srp.module_warnings
        .iter()
        .for_each(|w| push(&w.file, 1, Dimension::Srp));
    srp.param_warnings
        .iter()
        .for_each(|w| push(&w.file, w.line, Dimension::Srp));
}

/// Positions for Test-Quality warnings.
/// Operation: iterates TQ warnings pushing each entry.
fn collect_tq_positions<F>(analysis: &crate::report::AnalysisResult, push: &mut F)
where
    F: FnMut(&str, usize, crate::findings::Dimension),
{
    use crate::findings::Dimension;
    let Some(tq) = &analysis.tq else { return };
    tq.warnings
        .iter()
        .for_each(|w| push(&w.file, w.line, Dimension::TestQuality));
}

/// Positions for Structural binary-check warnings; each carries its
/// own mapped dimension (SRP or Coupling).
/// Operation: iterates structural warnings pushing each entry.
fn collect_structural_positions<F>(analysis: &crate::report::AnalysisResult, push: &mut F)
where
    F: FnMut(&str, usize, crate::findings::Dimension),
{
    let Some(st) = &analysis.structural else {
        return;
    };
    st.warnings
        .iter()
        .for_each(|w| push(&w.file, w.line, w.dimension));
}

/// Positions for Architecture-dimension findings.
/// Operation: iterates architecture findings pushing each entry.
fn collect_architecture_positions<F>(analysis: &crate::report::AnalysisResult, push: &mut F)
where
    F: FnMut(&str, usize, crate::findings::Dimension),
{
    use crate::findings::Dimension;
    analysis
        .architecture_findings
        .iter()
        .for_each(|f| push(&f.file, f.line, Dimension::Architecture));
}
