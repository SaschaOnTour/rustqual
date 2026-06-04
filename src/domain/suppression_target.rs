//! Suppression *targets* — the vocabulary that lets `// qual:allow(dim, …)`
//! silence ONE finding-kind within a dimension instead of the whole
//! dimension. A target is named by its config field (for threshold metrics)
//! or its rule name (for boolean findings):
//!
//! - **Metric** targets carry a numeric threshold, so a pin value is
//!   *mandatory*: `allow(srp, file_length=400)`. The suppression holds only
//!   while the metric stays at or below the pin, and re-fires above it.
//! - **Boolean** targets are yes/no findings, so they take *no* value:
//!   `allow(complexity, unsafe)`.
//!
//! This module is the single source of truth for which targets exist; the
//! per-dimension marking code consumes the same names.

use crate::domain::Dimension;

/// Whether a target carries a numeric threshold (a pin value is mandatory)
/// or is a yes/no finding-kind (no value allowed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    /// Threshold metric — `allow(dim, name=N)` requires the value `N`.
    Metric,
    /// Boolean finding-kind — `allow(dim, name)` takes no value.
    Boolean,
}

/// A parsed `allow(dim, target[, =N])` target: the finding-kind name plus,
/// for metric targets, the pinned ceiling. `pin` is `Some` exactly when the
/// target is a metric.
#[derive(Debug, Clone, PartialEq)]
pub struct SuppressionTarget {
    /// Config field name (metric) or rule name (boolean).
    pub name: String,
    /// Pinned ceiling for metric targets; `None` for boolean targets.
    pub pin: Option<f64>,
}

/// Resolve a `(dimension, target-name)` pair to its kind, or `None` when the
/// name is not a known target of that dimension. Keys are config field names
/// (metrics) and rule names (booleans).
pub fn target_kind(dim: Dimension, name: &str) -> Option<TargetKind> {
    use Dimension as D;
    use TargetKind::{Boolean, Metric};
    match dim {
        D::Complexity => match name {
            "max_cognitive" | "max_cyclomatic" | "max_nesting_depth" | "max_function_lines" => {
                Some(Metric)
            }
            "magic_numbers" | "error_handling" | "unsafe" => Some(Boolean),
            _ => None,
        },
        D::Srp => match name {
            "file_length" | "max_independent_clusters" | "max_parameters" => Some(Metric),
            "god_struct" => Some(Boolean),
            _ => None,
        },
        D::Coupling => match name {
            "max_fan_in" | "max_fan_out" | "max_instability" => Some(Metric),
            // `cycle` is NOT here: circular dependencies are not suppressible
            // via allow(coupling) (they always count toward the score).
            "sdp" => Some(Boolean),
            _ => None,
        },
        D::Dry => match name {
            // dead_code is NOT here: DRY-002 is excluded via `qual:api` /
            // `qual:test_helper`, not via `allow(dry)`.
            "duplicate" | "fragment" | "boilerplate" | "wildcard_imports" | "repeated_matches" => {
                Some(Boolean)
            }
            _ => None,
        },
        // Architecture and test-quality targets are added in their slices;
        // iosp is single-kind, so only the bare `allow(iosp)` form exists.
        D::Iosp | D::Architecture | D::TestQuality => None,
    }
}

/// All valid target names for a dimension, used to build the "unknown target"
/// error so the author sees exactly what they may write (this is what stops
/// the agent inventing a non-existent target like `srp_params`).
pub fn target_names(dim: Dimension) -> &'static [&'static str] {
    use Dimension as D;
    match dim {
        D::Complexity => &[
            "max_cognitive",
            "max_cyclomatic",
            "max_nesting_depth",
            "max_function_lines",
            "magic_numbers",
            "error_handling",
            "unsafe",
        ],
        D::Srp => &[
            "file_length",
            "max_independent_clusters",
            "max_parameters",
            "god_struct",
        ],
        D::Coupling => &["max_fan_in", "max_fan_out", "max_instability", "sdp"],
        D::Dry => &[
            "duplicate",
            "fragment",
            "boilerplate",
            "wildcard_imports",
            "repeated_matches",
        ],
        D::Iosp | D::Architecture | D::TestQuality => &[],
    }
}
