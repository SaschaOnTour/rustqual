//! Contract tests for the sealed `Reporter` port.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::domain::analysis_data::{FunctionRecord, ModuleCouplingRecord};
use crate::domain::findings::{
    ArchitectureFinding, ComplexityFinding, CouplingFinding, DryFinding, IospFinding,
    OrphanSuppression, SrpFinding, TqFinding,
};
use crate::domain::{AnalysisData, AnalysisFindings};
use crate::ports::reporter::{ReporterImpl, Snapshot};
use crate::ports::{ReportError, Reporter};

/// Counts how many times each `build_*` method fired. Uses atomics so
/// the reporter can be `Send + Sync` (the `ReporterImpl` trait requires
/// it) while still mutating through `&self` (port_traits forbids
/// `&mut self`).
#[derive(Default)]
struct CountingReporter {
    iosp: AtomicUsize,
    complexity: AtomicUsize,
    dry: AtomicUsize,
    srp: AtomicUsize,
    coupling: AtomicUsize,
    test_quality: AtomicUsize,
    architecture: AtomicUsize,
    orphans: AtomicUsize,
    iosp_data: AtomicUsize,
    complexity_data: AtomicUsize,
    coupling_data: AtomicUsize,
    publish_called: AtomicBool,
}

fn bump(c: &AtomicUsize) {
    c.fetch_add(1, Ordering::SeqCst);
}

impl ReporterImpl for CountingReporter {
    type Output = String;

    type IospView = &'static str;
    type ComplexityView = &'static str;
    type DryView = &'static str;
    type SrpView = &'static str;
    type CouplingView = &'static str;
    type TestQualityView = &'static str;
    type ArchitectureView = &'static str;
    type OrphanView = &'static str;
    type IospDataView = &'static str;
    type ComplexityDataView = &'static str;
    type CouplingDataView = &'static str;

    fn build_iosp(&self, _: &[IospFinding]) -> &'static str {
        bump(&self.iosp);
        "iosp"
    }
    fn build_complexity(&self, _: &[ComplexityFinding]) -> &'static str {
        bump(&self.complexity);
        "complexity"
    }
    fn build_dry(&self, _: &[DryFinding]) -> &'static str {
        bump(&self.dry);
        "dry"
    }
    fn build_srp(&self, _: &[SrpFinding]) -> &'static str {
        bump(&self.srp);
        "srp"
    }
    fn build_coupling(&self, _: &[CouplingFinding]) -> &'static str {
        bump(&self.coupling);
        "coupling"
    }
    fn build_test_quality(&self, _: &[TqFinding]) -> &'static str {
        bump(&self.test_quality);
        "test_quality"
    }
    fn build_architecture(&self, _: &[ArchitectureFinding]) -> &'static str {
        bump(&self.architecture);
        "architecture"
    }
    fn build_orphans(&self, _: &[OrphanSuppression]) -> &'static str {
        bump(&self.orphans);
        "orphans"
    }
    fn build_iosp_data(&self, _: &[FunctionRecord]) -> &'static str {
        bump(&self.iosp_data);
        "iosp_data"
    }
    fn build_complexity_data(&self, _: &[FunctionRecord]) -> &'static str {
        bump(&self.complexity_data);
        "complexity_data"
    }
    fn build_coupling_data(&self, _: &[ModuleCouplingRecord]) -> &'static str {
        bump(&self.coupling_data);
        "coupling_data"
    }

    fn publish(&self, snapshot: Snapshot<Self>) -> String {
        self.publish_called.store(true, Ordering::SeqCst);
        // Concatenate all 10 view tags in canonical order so a downstream
        // test can verify each view reached publish.
        let Snapshot {
            iosp,
            complexity,
            dry,
            srp,
            coupling,
            test_quality,
            architecture,
            orphans,
            iosp_data,
            complexity_data,
            coupling_data,
        } = snapshot;
        [
            iosp,
            complexity,
            dry,
            srp,
            coupling,
            test_quality,
            architecture,
            orphans,
            iosp_data,
            complexity_data,
            coupling_data,
        ]
        .join("|")
    }
}

#[test]
fn render_calls_every_build_method_exactly_once() {
    let reporter = CountingReporter::default();
    let findings = AnalysisFindings::default();
    let data = AnalysisData::default();
    reporter.render(&findings, &data);
    let load = |c: &AtomicUsize| c.load(Ordering::SeqCst);
    assert_eq!(load(&reporter.iosp), 1, "build_iosp must run once");
    assert_eq!(
        load(&reporter.complexity),
        1,
        "build_complexity must run once",
    );
    assert_eq!(load(&reporter.dry), 1, "build_dry must run once");
    assert_eq!(load(&reporter.srp), 1, "build_srp must run once");
    assert_eq!(load(&reporter.coupling), 1, "build_coupling must run once");
    assert_eq!(
        load(&reporter.test_quality),
        1,
        "build_test_quality must run once",
    );
    assert_eq!(
        load(&reporter.architecture),
        1,
        "build_architecture must run once",
    );
    assert_eq!(
        load(&reporter.orphans),
        1,
        "build_orphans must run once — compile-time guarantee for orphan-suppression rendering across all reporters",
    );
    assert_eq!(
        load(&reporter.iosp_data),
        1,
        "build_iosp_data must run once",
    );
    assert_eq!(
        load(&reporter.complexity_data),
        1,
        "build_complexity_data must run once",
    );
    assert_eq!(
        load(&reporter.coupling_data),
        1,
        "build_coupling_data must run once",
    );
    assert!(
        reporter.publish_called.load(Ordering::SeqCst),
        "publish must be called",
    );
}

#[test]
fn render_passes_views_to_publish_in_canonical_order() {
    let reporter = CountingReporter::default();
    let out = reporter.render(&AnalysisFindings::default(), &AnalysisData::default());
    assert_eq!(
        out,
        "iosp|complexity|dry|srp|coupling|test_quality|architecture|orphans|\
         iosp_data|complexity_data|coupling_data",
        "publish must receive all 11 views, snapshot fields populated by build_* in canonical order",
    );
}

#[test]
fn reporter_trait_requires_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<CountingReporter>();
}

#[test]
fn report_error_variants_carry_diagnostic_information() {
    let e = ReportError::Io("broken pipe".into());
    assert!(e.to_string().contains("broken pipe"));

    let e = ReportError::Encoding("invalid utf-8".into());
    assert!(e.to_string().contains("invalid utf-8"));
}
