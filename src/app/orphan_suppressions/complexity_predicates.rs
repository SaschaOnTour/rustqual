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
    test_max_lines: usize,
) -> bool {
    exceeds_basic_thresholds(c, cx)
        || exceeds_length(f, c, cx, test_max_lines)
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
