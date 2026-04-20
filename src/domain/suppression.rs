//! Suppression value objects.
//!
//! A `Suppression` is the parsed, framework-free representation of a
//! `// qual:allow(…)` comment (or the legacy `// iosp:allow` form).
//! The actual comment parsing lives in the suppression-adapter
//! (`crate::findings` today, `src/adapters/suppression/` after Phase 4).

use crate::domain::Dimension;

/// A parsed suppression that applies on a specific source line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suppression {
    /// Line number where the suppression comment appears (1-based).
    pub line: usize,
    /// Which dimensions to suppress. Empty means suppress all dimensions.
    pub dimensions: Vec<Dimension>,
    /// Optional human-readable reason.
    pub reason: Option<String>,
}

impl Suppression {
    /// Check if this suppression covers a given dimension.
    /// An empty `dimensions` list covers all dimensions.
    pub fn covers(&self, dim: Dimension) -> bool {
        self.dimensions.is_empty() || self.dimensions.contains(&dim)
    }
}
