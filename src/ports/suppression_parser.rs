//! Port: extract suppression directives from source files.

use crate::domain::{SourceUnit, Suppression};

/// Parses `qual:allow(...)` comments (and legacy `iosp:allow`) from a
/// `SourceUnit` into Domain `Suppression` values.
pub trait SuppressionParser: Send + Sync {
    /// Parse all suppressions from one source unit.
    ///
    /// Returns an empty Vec if the file contains none — not an error.
    fn parse(&self, unit: &SourceUnit) -> Result<Vec<Suppression>, SuppressionParseError>;
}

/// Errors a `SuppressionParser` may report.
#[derive(Debug, thiserror::Error)]
pub enum SuppressionParseError {
    #[error("malformed suppression at {file}:{line}: {message}")]
    Malformed {
        file: String,
        line: usize,
        message: String,
    },
    #[error("unknown dimension referenced at {file}:{line}: {dimension}")]
    UnknownDimension {
        file: String,
        line: usize,
        dimension: String,
    },
}
