//! Per-dimension typed state-of-codebase records.
//!
//! Counterpart to `domain::findings`. Where `AnalysisFindings`
//! carries _what's wrong_ (per-dimension `Finding` lists),
//! `AnalysisData` carries _what is_: per-function records (consumed
//! by the IOSP and Complexity dimensions) and per-module coupling
//! records (consumed by the Coupling dimension).
//!
//! Reporter-Trait split mirrors this: every Reporter consumes
//! `AnalysisFindings`; only `AnalysisReporter` (which extends
//! `Reporter`) additionally consumes `AnalysisData`. Findings-only
//! reporters (AI, github, findings_list, SARIF, text-compact)
//! implement just `Reporter`. Analyse-aware reporters (JSON, HTML,
//! text-verbose, dot) implement `Reporter` + `AnalysisReporter`.
//!
//! Only dimensions with **actual** state-of-codebase data have a
//! data record / data method here. DRY, SRP, TQ and Architecture
//! have no data methods today — when a future need surfaces, adding
//! the data record + trait method causes every existing
//! `AnalysisReporter` impl to fail compilation, forcing a conscious
//! decision per reporter. Empty placeholders would defeat that
//! guarantee.

pub mod aggregate;
pub mod coupling_record;
pub mod function_record;

pub use aggregate::AnalysisData;
pub use coupling_record::ModuleCouplingRecord;
pub use function_record::{
    ComplexityMetricsRecord, FunctionClassification, FunctionRecord, LogicOccurrenceRecord,
    MagicNumberOccurrence, NestingHotspot,
};
