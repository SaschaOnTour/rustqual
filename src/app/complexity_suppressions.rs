//! Per-kind targeted suppression for the complexity dimension.
//!
//! A blanket `allow(complexity)` is handled by
//! `FunctionAnalysis::complexity_suppressed` (set in
//! `warnings::apply_file_suppressions`, blanket-only). This module decides
//! whether a *targeted* `allow(complexity, <kind>[=N])` silences one specific
//! kind: `max_cognitive` / `max_cyclomatic` / `max_nesting_depth` /
//! `max_function_lines` (metric pins) or `magic_numbers` / `error_handling` /
//! `unsafe` (boolean). Keeping it separate keeps `warnings.rs` focused.

use std::collections::HashMap;

use crate::adapters::analyzers::iosp::FunctionAnalysis;
use crate::findings::{Dimension, Suppression};

/// True when a targeted complexity suppression adjacent to `fa` silences the
/// `target` kind at metric `value` (`None` for boolean kinds). Blanket
/// suppressions are excluded here — they are handled upstream via
/// `complexity_suppressed`.
/// Operation: file lookup + adjacency/pin check via closures.
pub(crate) fn cx_target_suppressed(
    suppression_lines: &HashMap<String, Vec<Suppression>>,
    fa: &FunctionAnalysis,
    target: &str,
    value: Option<f64>,
) -> bool {
    let window = crate::findings::ANNOTATION_WINDOW;
    suppression_lines.get(&fa.file).is_some_and(|sups| {
        sups.iter().any(|s| {
            let adjacent = s.line <= fa.line && fa.line - s.line <= window;
            adjacent && s.target.is_some() && s.suppresses(Dimension::Complexity, target, value)
        })
    })
}

/// Whether a metric kind warns: it `exceeds` its threshold and is not silenced
/// by a targeted pin honouring `value`. Keeps the `&&` and the lookup out of
/// the warning passes so those stay simple.
/// Operation: boolean combination, own call hidden behind the `&&`.
pub(crate) fn metric_warns(
    suppression_lines: &HashMap<String, Vec<Suppression>>,
    fa: &FunctionAnalysis,
    exceeds: bool,
    value: f64,
    target: &str,
) -> bool {
    exceeds && !cx_target_suppressed(suppression_lines, fa, target, Some(value))
}

/// Whether a boolean kind warns: the issue is `present` and not silenced by a
/// targeted `allow(complexity, <target>)`.
/// Operation: boolean combination.
pub(crate) fn bool_warns(
    suppression_lines: &HashMap<String, Vec<Suppression>>,
    fa: &FunctionAnalysis,
    present: bool,
    target: &str,
) -> bool {
    present && !cx_target_suppressed(suppression_lines, fa, target, None)
}
