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

/// A parsed `allow(dim, target[, =N])` target. Modeled as a sum type so the
/// "metric ⇒ has a pin, boolean ⇒ has no pin" invariant is unrepresentable
/// otherwise: there is no way to build a `Boolean` carrying a pin or a
/// `Metric` without one. Construct via the parser; read via `name()`/`pin()`.
#[derive(Debug, Clone, PartialEq)]
pub enum SuppressionTarget {
    /// Threshold metric (config field name) with its mandatory pinned ceiling.
    /// Silences only while the finding's value is `<= pin`.
    Metric { name: String, pin: f64 },
    /// Boolean finding-kind (rule name); silences by name, carries no value.
    Boolean { name: String },
}

impl SuppressionTarget {
    /// The target's name (config field for metrics, rule name for booleans).
    pub fn name(&self) -> &str {
        match self {
            Self::Metric { name, .. } | Self::Boolean { name } => name,
        }
    }

    /// The pinned ceiling for a metric target; `None` for a boolean target.
    pub fn pin(&self) -> Option<f64> {
        match self {
            Self::Metric { pin, .. } => Some(*pin),
            Self::Boolean { .. } => None,
        }
    }
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
        D::Architecture => match name {
            // The top-level rule family (the `architecture/<family>/…` segment).
            "call_parity" | "forbidden" | "layer" | "pattern" | "trait_contract" => Some(Boolean),
            _ => None,
        },
        D::TestQuality => match name {
            "no_assertion" | "no_sut" | "untested" | "uncovered" | "untested_logic" => {
                Some(Boolean)
            }
            _ => None,
        },
        // iosp is single-kind, so only the bare `allow(iosp)` form exists.
        D::Iosp => None,
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
        D::Architecture => &[
            "call_parity",
            "forbidden",
            "layer",
            "pattern",
            "trait_contract",
        ],
        D::TestQuality => &[
            "no_assertion",
            "no_sut",
            "untested",
            "uncovered",
            "untested_logic",
        ],
        D::Iosp => &[],
    }
}
