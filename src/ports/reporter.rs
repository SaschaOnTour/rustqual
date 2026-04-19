//! Port: render analysis output.
//!
//! Reporters receive the finalised analysis result and write it — to
//! stdout, a file, a SARIF envelope, an HTML dashboard. Each output
//! format is an Adapter implementing this port.

/// A reporter emits analysis results in its format-specific way.
///
/// The concrete payload type is intentionally opaque at this phase —
/// the Application layer hasn't yet produced a consolidated `Finding`
/// aggregate, and designing a large payload struct now would be
/// speculative. The full payload type lands when Phase 5 moves the
/// report dispatcher into the Application layer.
pub trait Reporter: Send + Sync {
    /// Emit the finished report. The `payload` is currently unspecified
    /// (Phase 5 introduces a proper `ReportPayload` Domain type).
    fn emit(&self, payload: &ReportPayload) -> Result<(), ReportError>;
}

/// Placeholder for the finished analysis payload.
///
/// In Phase 5, this becomes a concrete Domain type aggregating Finding,
/// QualityScore and diagnostic metadata. For now it carries a sentinel
/// string so the port compiles and can be mocked by tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportPayload {
    /// Placeholder content — replaced with structured Finding data in Phase 5.
    pub placeholder: String,
}

/// Errors that a reporter may report.
#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("i/o error writing report: {0}")]
    Io(String),
    #[error("encoding error: {0}")]
    Encoding(String),
}
