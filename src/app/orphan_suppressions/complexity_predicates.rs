//! Config-gated predicates mirroring `apply_extended_warnings`.
//!
//! These helpers tell the orphan checker whether a function's raw
//! complexity metrics would trigger a warning under the active
//! config — needed because a `// qual:allow(complexity)` marker
//! clears the `*_warning` flags on the `FunctionAnalysis` before the
//! orphan pass sees it. Reading raw metrics + config lets us
//! recognize those markers as non-orphan.

use crate::adapters::analyzers::iosp::{ComplexityMetrics, FunctionAnalysis};
use crate::config::sections::ComplexityConfig;

/// True if the raw complexity metrics of a function would trigger any
/// complexity warning under the active config.
/// Integration: delegates to per-aspect predicates.
pub(super) fn would_trigger(
    f: &FunctionAnalysis,
    c: &ComplexityMetrics,
    cx: &ComplexityConfig,
) -> bool {
    exceeds_basic_thresholds(c, cx)
        || exceeds_length(f, c, cx)
        || exceeds_unsafe(c, cx)
        || exceeds_error_handling(f, c, cx)
}

/// True if cognitive / cyclomatic / nesting exceed their thresholds.
/// Operation: comparison logic.
fn exceeds_basic_thresholds(c: &ComplexityMetrics, cx: &ComplexityConfig) -> bool {
    c.cognitive_complexity > cx.max_cognitive
        || c.cyclomatic_complexity > cx.max_cyclomatic
        || c.max_nesting > cx.max_nesting_depth
}

/// True if the function (production, not test) exceeds the length cap.
/// Operation: comparison logic.
fn exceeds_length(f: &FunctionAnalysis, c: &ComplexityMetrics, cx: &ComplexityConfig) -> bool {
    !f.is_test && c.function_lines > cx.max_function_lines
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
