//! Contract tests for the `SuppressionParser` port.

use crate::domain::{Dimension, SourceUnit, Suppression};
use crate::ports::{SuppressionParseError, SuppressionParser};
use std::path::PathBuf;

#[cfg(test)]
struct FakeParser {
    returns: Vec<Suppression>,
}

#[cfg(test)]
impl SuppressionParser for FakeParser {
    fn parse(&self, _unit: &SourceUnit) -> Result<Vec<Suppression>, SuppressionParseError> {
        Ok(self.returns.clone())
    }
}

#[test]
fn port_is_object_safe() {
    let _boxed: Box<dyn SuppressionParser> = Box::new(FakeParser { returns: vec![] });
}

#[test]
fn port_requires_send_and_sync() {
    let _: Box<dyn Send + Sync> = Box::new(FakeParser { returns: vec![] });
}

#[test]
fn fake_parser_returns_injected_suppressions() {
    const FIXTURE_LINE: usize = 10;
    let sup = Suppression {
        line: FIXTURE_LINE,
        dimensions: vec![Dimension::Architecture],
        reason: Some("migration".into()),
    };
    let parser = FakeParser {
        returns: vec![sup.clone()],
    };
    let unit = SourceUnit::new(PathBuf::from("x.rs"), "".into());
    let parsed = parser.parse(&unit).unwrap();
    assert_eq!(parsed, vec![sup]);
}

// qual:allow(test_quality) reason: "contract test verifying Display on Error variants does not call a SUT method by design"
#[test]
fn parse_error_variants_carry_diagnostic_information() {
    let e = SuppressionParseError::Malformed {
        file: "x.rs".into(),
        line: 5,
        message: "bad syntax".into(),
    };
    let s = e.to_string();
    assert!(s.contains("x.rs"));
    assert!(s.contains("5"));
    assert!(s.contains("bad syntax"));

    let e = SuppressionParseError::UnknownDimension {
        file: "y.rs".into(),
        line: 12,
        dimension: "quality".into(),
    };
    let s = e.to_string();
    assert!(s.contains("y.rs"));
    assert!(s.contains("quality"));
}
