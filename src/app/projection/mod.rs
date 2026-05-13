//! Project per-dimension analyzer output into typed `Finding` shapes.
//!
//! The pipeline produces dimension-specific Reports (DuplicateGroup,
//! SrpAnalysis, CouplingAnalysis, …); reporters consume the unified
//! `AnalysisFindings` aggregate. This module is the bridge: pure
//! transformations from dimension-specific data into typed Findings.
//!
//! Each `project_<dim>` function takes the dimension's existing
//! analyzer output and returns a `Vec<*Finding>` suitable for
//! `AnalysisFindings.<dim>`. Per-dimension rule-id and severity
//! conventions are encoded in the per-dimension submodules.
//!
//! v1.2.1 transition: legacy dimension-specific fields on
//! `AnalysisResult` are kept alongside the typed `findings` aggregate
//! while reporters migrate to the Reporter trait.

mod architecture;
mod complexity;
mod coupling;
mod data;
mod dry;
mod iosp;
mod srp;
mod structural_shared;
mod tq;

pub(crate) use architecture::project_architecture;
pub(crate) use complexity::project_complexity;
pub(crate) use coupling::project_coupling;
pub(crate) use data::project_data;
pub(crate) use dry::project_dry;
pub(crate) use iosp::project_iosp;
pub(crate) use srp::project_srp;
pub(crate) use tq::project_tq;
