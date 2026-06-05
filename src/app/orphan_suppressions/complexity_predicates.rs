//! Config-gated predicates mirroring `apply_extended_warnings`.
//!
//! These helpers tell the orphan checker which suppression-targets a
//! function's raw complexity metrics trip under the active config — needed
//! because a `// qual:allow(complexity, …)` marker clears the `*_warning`
//! flags on the `FunctionAnalysis` before the orphan pass sees it. Reading
//! raw metrics + config lets us recognize those markers as non-orphan, and
//! lets the too-loose-pin check compare a pin against the actual value.

use crate::adapters::analyzers::iosp::{ComplexityMetrics, FunctionAnalysis};
use crate::config::sections::ComplexityConfig;

/// The complexity suppression-targets a function trips under the active
/// config, each paired with its raw value (`None` for the boolean targets
/// `unsafe` / `error_handling`). Drives orphan target-matching and the
/// too-loose-pin check: a `allow(complexity, max_cognitive=…)` marker is
/// non-stale only when `max_cognitive` is in this list, and its pin is
/// judged against the paired value. Magic numbers are handled separately
/// (per-occurrence lines), not here.
/// Operation: per-aspect threshold checks collecting (target, value) pairs.
pub(super) fn triggered_targets(
    f: &FunctionAnalysis,
    c: &ComplexityMetrics,
    cx: &ComplexityConfig,
    test_max_lines: usize,
) -> Vec<(&'static str, Option<f64>)> {
    let mut out = Vec::new();
    if c.cognitive_complexity > cx.max_cognitive {
        out.push(("max_cognitive", Some(c.cognitive_complexity as f64)));
    }
    if c.cyclomatic_complexity > cx.max_cyclomatic {
        out.push(("max_cyclomatic", Some(c.cyclomatic_complexity as f64)));
    }
    if c.max_nesting > cx.max_nesting_depth {
        out.push(("max_nesting_depth", Some(c.max_nesting as f64)));
    }
    if exceeds_length(f, c, cx, test_max_lines) {
        out.push(("max_function_lines", Some(c.function_lines as f64)));
    }
    if exceeds_unsafe(c, cx) {
        out.push(("unsafe", None));
    }
    if exceeds_error_handling(f, c, cx) {
        out.push(("error_handling", None));
    }
    out
}

/// True if the function exceeds its length cap — test fns use `test_max_lines`
/// (`[tests].max_function_lines`, defaulting to production), production fns use
/// `[complexity].max_function_lines`. Mirrors `warnings::is_length_over`.
/// Operation: threshold selection + comparison.
fn exceeds_length(
    f: &FunctionAnalysis,
    c: &ComplexityMetrics,
    cx: &ComplexityConfig,
    test_max_lines: usize,
) -> bool {
    let max = if f.is_test {
        test_max_lines
    } else {
        cx.max_function_lines
    };
    c.function_lines > max
}

/// True if unsafe detection is enabled and the function contains at
/// least one unsafe block.
/// Operation: comparison logic.
fn exceeds_unsafe(c: &ComplexityMetrics, cx: &ComplexityConfig) -> bool {
    cx.detect_unsafe && c.unsafe_blocks > 0
}

/// True if error-handling detection is enabled and the (production)
/// function uses any of unwrap/panic/todo/(expect unless allowed).
/// Operation: comparison logic.
fn exceeds_error_handling(
    f: &FunctionAnalysis,
    c: &ComplexityMetrics,
    cx: &ComplexityConfig,
) -> bool {
    if !cx.detect_error_handling || f.is_test {
        return false;
    }
    let expect_threshold = if cx.allow_expect { 0 } else { 1 };
    c.unwrap_count + c.panic_count + c.todo_count + c.expect_count.min(expect_threshold) > 0
}
