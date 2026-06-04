//! Suppression value objects.
//!
//! A `Suppression` is the parsed, framework-free representation of a
//! `// qual:allow(…)` comment (or the legacy `// iosp:allow` form).
//! The actual comment parsing lives in the suppression-adapter
//! (`crate::findings` today, `src/adapters/suppression/` after Phase 4).

use crate::domain::suppression_target::SuppressionTarget;
use crate::domain::Dimension;

/// A parsed suppression that applies on a specific source line.
#[derive(Debug, Clone, PartialEq)]
pub struct Suppression {
    /// Line number where the suppression comment appears (1-based).
    pub line: usize,
    /// Which dimensions to suppress. Empty means suppress all dimensions.
    pub dimensions: Vec<Dimension>,
    /// Optional human-readable reason.
    pub reason: Option<String>,
    /// When `Some`, the suppression targets ONE finding-kind within its
    /// (single) dimension — `allow(srp, file_length=400)` — rather than the
    /// whole dimension. `None` is the blanket `allow(dim)` form.
    pub target: Option<SuppressionTarget>,
}

impl Suppression {
    /// Construct a blanket (whole-dimension) suppression — the `allow(dim)`
    /// form with no target. Convenience for the many call sites that predate
    /// targeted suppressions.
    pub fn blanket(line: usize, dimensions: Vec<Dimension>, reason: Option<String>) -> Self {
        Self {
            line,
            dimensions,
            reason,
            target: None,
        }
    }

    /// Check if this suppression covers a given dimension.
    /// An empty `dimensions` list covers all dimensions.
    pub fn covers(&self, dim: Dimension) -> bool {
        self.dimensions.is_empty() || self.dimensions.contains(&dim)
    }

    /// Whether this suppression silences `dim`'s `target_name` finding at the
    /// given metric `value`. A blanket suppression (no target) silences every
    /// finding of its dimension. A metric pin silences only while
    /// `value <= pin` — above the pin it re-fires; a boolean target silences
    /// by name. `value` is `None` for findings that carry no metric.
    /// Operation: dimension + target dispatch, no own calls beyond `covers`.
    pub fn suppresses(&self, dim: Dimension, target_name: &str, value: Option<f64>) -> bool {
        if !self.covers(dim) {
            return false;
        }
        match &self.target {
            None => true,
            Some(t) if t.name == target_name => match (t.pin, value) {
                (Some(pin), Some(v)) => v <= pin,
                (None, _) => true,
                (Some(_), None) => false,
            },
            Some(_) => false,
        }
    }
}
