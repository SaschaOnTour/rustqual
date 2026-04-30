//! Port: render analysis output, dimension by dimension.
//!
//! Sealed two-trait pattern:
//! - [`Reporter`] is the single public entry point: `render(findings,
//!   data) -> Output`. External crates cannot implement it (the
//!   [`sealed::Sealed`] supertrait lives in a private module).
//! - [`ReporterImpl`] is the contract every concrete reporter satisfies
//!   internally: per-dim `build_*` projections + a final `publish` that
//!   composes the per-dim [`Snapshot`] into the reporter's `Output`.
//! - [`Snapshot`] aggregates the ten per-dim views; its fields are
//!   `pub(crate)` so external code cannot construct one and therefore
//!   cannot reach `publish` directly.
//!
//! Adding a new dimension means: extend [`ReporterImpl`] with a `type
//! FooView` + `fn build_foo`, add `foo` to [`Snapshot`], and add the
//! corresponding line to the blanket `render` constructor below. Every
//! existing reporter then fails to compile until it adds the
//! corresponding pieces — the compile-time Reporter-Parity guarantee.

use crate::domain::analysis_data::{FunctionRecord, ModuleCouplingRecord};
use crate::domain::findings::{
    ArchitectureFinding, ComplexityFinding, CouplingFinding, DryFinding, IospFinding, SrpFinding,
    TqFinding,
};

mod sealed {
    /// Marker trait that prevents external crates from implementing
    /// [`super::Reporter`] directly. Only types in this crate that
    /// implement [`super::ReporterImpl`] receive a `Sealed` impl
    /// (via the blanket below), which means only crate-internal
    /// reporters can satisfy the `Reporter` bound.
    pub trait Sealed: Send + Sync {}
}

/// Public reporter facade. The single polymorphic entry point: callers
/// invoke `render()` and receive the reporter's native `Output`.
///
/// External code outside the rustqual crate cannot implement this trait
/// directly — the [`sealed::Sealed`] supertrait is in a private module.
/// To add a new reporter, implement [`ReporterImpl`]; the blanket impl
/// below derives [`Reporter`] automatically.
pub trait Reporter: sealed::Sealed + Send + Sync {
    /// The reporter's native final output type (e.g. `String`,
    /// `serde_json::Value`).
    type Output;

    /// Render the analysis into the reporter's output format. The
    /// canonical and only public entry point. Cannot be overridden by
    /// implementors (the blanket impl below provides the canonical
    /// orchestration over [`ReporterImpl`]).
    fn render(
        &self,
        findings: &crate::domain::AnalysisFindings,
        data: &crate::domain::AnalysisData,
    ) -> Self::Output;
}

/// Reporter implementation trait. Defines the per-dim projection
/// (`build_*`) and final composition (`publish`) methods that every
/// concrete reporter must provide.
///
/// **Encapsulation via sealed + Snapshot privacy.** This trait is `pub`
/// for technical reasons (its associated `Output` type is exposed via
/// the [`Reporter`] blanket impl), but external code is locked out of
/// the reporter pipeline in two ways:
///
/// 1. The [`sealed::Sealed`] supertrait is in a private module —
///    external crates cannot implement [`ReporterImpl`].
/// 2. [`Snapshot`] has `pub(crate)` fields — external code cannot
///    construct a `Snapshot<R>` and therefore cannot call
///    [`ReporterImpl::publish`] regardless of trait visibility.
///
/// The only path from outside the crate is [`Reporter::render`].
///
/// Adding a new dimension means: add the `type FooView` alias, add the
/// `fn build_foo` method, add `foo` to [`Snapshot`]. Every existing
/// reporter then fails to compile until it adds the corresponding
/// pieces — that's the compile-time Reporter-Parity guarantee.
pub trait ReporterImpl: Sized + sealed::Sealed + Send + Sync {
    type Output;

    type IospView;
    type ComplexityView;
    type DryView;
    type SrpView;
    type CouplingView;
    type TestQualityView;
    type ArchitectureView;
    type IospDataView;
    type ComplexityDataView;
    type CouplingDataView;

    fn build_iosp(&self, findings: &[IospFinding]) -> Self::IospView;
    fn build_complexity(&self, findings: &[ComplexityFinding]) -> Self::ComplexityView;
    fn build_dry(&self, findings: &[DryFinding]) -> Self::DryView;
    fn build_srp(&self, findings: &[SrpFinding]) -> Self::SrpView;
    fn build_coupling(&self, findings: &[CouplingFinding]) -> Self::CouplingView;
    fn build_test_quality(&self, findings: &[TqFinding]) -> Self::TestQualityView;
    fn build_architecture(&self, findings: &[ArchitectureFinding]) -> Self::ArchitectureView;
    fn build_iosp_data(&self, fns: &[FunctionRecord]) -> Self::IospDataView;
    fn build_complexity_data(&self, fns: &[FunctionRecord]) -> Self::ComplexityDataView;
    fn build_coupling_data(&self, mods: &[ModuleCouplingRecord]) -> Self::CouplingDataView;

    /// Compose the per-dim views into the reporter's final output.
    /// Reporters destructure the [`Snapshot`] exhaustively here — that
    /// destructuring is the third compile-time guard (in addition to
    /// the trait method set and the snapshot constructor in the blanket
    /// `render` impl).
    fn publish(&self, snapshot: Snapshot<Self>) -> Self::Output;
}

/// All ten per-dim views aggregated, ready for `publish` to consume.
/// Generic over the implementing reporter so each `R::*View` is the
/// reporter's own projection type.
///
/// Fields are `pub(crate)` so external code cannot construct a Snapshot
/// — that's what keeps [`ReporterImpl::publish`] effectively
/// crate-internal even though the trait itself is `pub`.
pub struct Snapshot<R: ReporterImpl> {
    pub(crate) iosp: R::IospView,
    pub(crate) complexity: R::ComplexityView,
    pub(crate) dry: R::DryView,
    pub(crate) srp: R::SrpView,
    pub(crate) coupling: R::CouplingView,
    pub(crate) test_quality: R::TestQualityView,
    pub(crate) architecture: R::ArchitectureView,
    pub(crate) iosp_data: R::IospDataView,
    pub(crate) complexity_data: R::ComplexityDataView,
    pub(crate) coupling_data: R::CouplingDataView,
}

// Blanket Sealed: every ReporterImpl gets the marker, nothing else does.
impl<T: ReporterImpl> sealed::Sealed for T {}

// Blanket Reporter: every ReporterImpl automatically becomes a Reporter
// with the canonical `render` orchestration. The blanket cannot be
// overridden because `render` is not a method on `ReporterImpl`.
impl<T: ReporterImpl> Reporter for T {
    type Output = <T as ReporterImpl>::Output;

    fn render(
        &self,
        findings: &crate::domain::AnalysisFindings,
        data: &crate::domain::AnalysisData,
    ) -> Self::Output {
        let snapshot = Snapshot {
            iosp: self.build_iosp(&findings.iosp),
            complexity: self.build_complexity(&findings.complexity),
            dry: self.build_dry(&findings.dry),
            srp: self.build_srp(&findings.srp),
            coupling: self.build_coupling(&findings.coupling),
            test_quality: self.build_test_quality(&findings.test_quality),
            architecture: self.build_architecture(&findings.architecture),
            iosp_data: self.build_iosp_data(&data.functions),
            complexity_data: self.build_complexity_data(&data.functions),
            coupling_data: self.build_coupling_data(&data.modules),
        };
        self.publish(snapshot)
    }
}

/// Errors that a reporter may report when finalising output to a sink.
#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    /// Failed to write the finished report to its destination.
    #[error("i/o error writing report: {0}")]
    Io(String),
    /// The report could not be encoded into the target format.
    #[error("encoding error: {0}")]
    Encoding(String),
}
