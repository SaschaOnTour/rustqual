//! Analyzer adapters — one per quality dimension.
//!
//! Each submodule hosts the concrete logic for one dimension, structured
//! as its own adapter. In Phase 5, each adapter will implement the
//! `DimensionAnalyzer` port so the Application layer can iterate over
//! `&[Box<dyn DimensionAnalyzer>]` without knowing the concrete types.

pub mod architecture;
pub mod coupling;
pub mod dry;
pub mod iosp;
pub mod srp;
pub mod structural;
pub mod tq;
