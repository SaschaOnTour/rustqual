//! Use-case: run every dimension analyzer on one parsed workspace.
//!
//! This is the port-level orchestrator. The composition root picks which
//! concrete analyzers to wire in (architecture, iosp, dry, …); the use-case
//! iterates over them blindly through the `DimensionAnalyzer` trait object
//! and gathers all findings into one flat `Vec`.
//!
//! Adding a dimension is a composition-root change — no edit needed here.

use crate::domain::Finding;
use crate::ports::{AnalysisContext, DimensionAnalyzer};

/// Run every analyzer in `analyzers` against `ctx` and collect findings.
/// Operation: iterator-chain flat-map, no own calls.
pub fn analyze_codebase(
    analyzers: &[Box<dyn DimensionAnalyzer>],
    ctx: &AnalysisContext<'_>,
) -> Vec<Finding> {
    analyzers.iter().flat_map(|a| a.analyze(ctx)).collect()
}
