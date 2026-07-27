use crate::adapters::analyzers::tq::*;

#[test]
fn test_tq_analysis_default_empty() {
    let analysis = TqAnalysis::default();
    assert!(analysis.warnings.is_empty());
}

#[test]
fn test_tq_warning_kind_equality() {
    assert_eq!(TqWarningKind::NoAssertion, TqWarningKind::NoAssertion);
    assert_eq!(TqWarningKind::NoSut, TqWarningKind::NoSut);
    assert_eq!(
        TqWarningKind::Untested { measured: false },
        TqWarningKind::Untested { measured: false }
    );
    assert_eq!(TqWarningKind::Uncovered, TqWarningKind::Uncovered);
    assert_ne!(TqWarningKind::NoAssertion, TqWarningKind::NoSut);
}
