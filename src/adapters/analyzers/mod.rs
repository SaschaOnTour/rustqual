//! Analyzer adapters — one per quality dimension.
//!
//! Each submodule hosts the concrete logic for one dimension,
//! structured as its own adapter. The `architecture` adapter is wired
//! through the `DimensionAnalyzer` port and is consumed via
//! `app::analyze_codebase` as `&[Box<dyn DimensionAnalyzer>]`. The
//! remaining dimensions (iosp, complexity, dry, srp, coupling, tq,
//! structural) are still invoked directly by the pipeline for
//! performance reasons — their rich per-dimension output structs
//! (`FunctionAnalysis`, `DeadCodeWarning`, etc.) are consumed by the
//! text/HTML/JSON reporters without an intermediate Finding
//! projection.

pub mod architecture;
pub mod coupling;
pub mod dry;
pub mod iosp;
pub mod srp;
pub mod structural;
pub mod tq;
