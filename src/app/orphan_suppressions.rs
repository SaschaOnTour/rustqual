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
    F: FnMut(&str, usize, crate::findings::Dimension),
{
    use crate::findings::Dimension;
    analysis.results.iter().for_each(|f| {
        if matches!(f.classification, Classification::Violation { .. }) {
            push(&f.file, f.line, Dimension::Iosp);
        }
        if let Some(c) = &f.complexity {
            if would_trigger_complexity_warning(f, c, &config.complexity) {
                push(&f.file, f.line, Dimension::Complexity);
            }
            push_magic_numbers(f, c, &config.complexity, push);
        }
    });
}

/// True if the raw complexity metrics of a function would trigger any
/// complexity warning under the active config — mirrors the predicates
/// in `apply_extended_warnings` so `detect_unsafe`,
/// `detect_error_handling`, `allow_expect`, and `is_test` are all
/// honored. Used by the orphan checker to recognize
/// `// qual:allow(complexity)` markers as non-orphan even after the
/// suppression clears the `*_warning` flags on the `FunctionAnalysis`.
/// Integration: delegates to per-aspect predicates.
fn would_trigger_complexity_warning(
    f: &crate::adapters::analyzers::iosp::FunctionAnalysis,
    c: &crate::adapters::analyzers::iosp::ComplexityMetrics,
    cx: &crate::config::sections::ComplexityConfig,
) -> bool {
    exceeds_basic_thresholds(c, cx)
        || exceeds_length(f, c, cx)
        || exceeds_unsafe(c, cx)
        || exceeds_error_handling(f, c, cx)
}

/// True if cognitive / cyclomatic / nesting exceed their thresholds.
/// Operation: comparison logic.
fn exceeds_basic_thresholds(
    c: &crate::adapters::analyzers::iosp::ComplexityMetrics,
    cx: &crate::config::sections::ComplexityConfig,
) -> bool {
    c.cognitive_complexity > cx.max_cognitive
        || c.cyclomatic_complexity > cx.max_cyclomatic
        || c.max_nesting > cx.max_nesting_depth
}

/// True if the function (production, not test) exceeds the length cap.
/// Operation: comparison logic.
fn exceeds_length(
    f: &crate::adapters::analyzers::iosp::FunctionAnalysis,
    c: &crate::adapters::analyzers::iosp::ComplexityMetrics,
    cx: &crate::config::sections::ComplexityConfig,
) -> bool {
    !f.is_test && c.function_lines > cx.max_function_lines
}

/// True if unsafe detection is enabled and the function contains at
/// least one unsafe block.
/// Operation: comparison logic.
fn exceeds_unsafe(
    c: &crate::adapters::analyzers::iosp::ComplexityMetrics,
    cx: &crate::config::sections::ComplexityConfig,
) -> bool {
    cx.detect_unsafe && c.unsafe_blocks > 0
}

/// True if error-handling detection is enabled and the (production)
/// function uses any of unwrap/panic/todo/(expect unless allowed).
/// Operation: comparison logic.
fn exceeds_error_handling(
    f: &crate::adapters::analyzers::iosp::FunctionAnalysis,
    c: &crate::adapters::analyzers::iosp::ComplexityMetrics,
    cx: &crate::config::sections::ComplexityConfig,
) -> bool {
    if !cx.detect_error_handling || f.is_test {
        return false;
    }
    let expect_threshold = if cx.allow_expect { 0 } else { 1 };
    c.unwrap_count + c.panic_count + c.todo_count + c.expect_count.min(expect_threshold) > 0
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
    F: FnMut(&str, usize, crate::findings::Dimension),
{
    if f.is_test || !cx.detect_magic_numbers {
        return;
    }
    c.magic_numbers
        .iter()
        .for_each(|m| push(&f.file, m.line, crate::findings::Dimension::Complexity));
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
