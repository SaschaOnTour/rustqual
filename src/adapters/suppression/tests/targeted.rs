//! Tests for the targeted-suppression grammar: `allow(dim, target[, =N])`.
//! Metric targets require a value (no blind threshold suppression); boolean
//! targets reject a value; every targeted form requires a reason.

use crate::adapters::suppression::qual_allow::*;

#[test]
fn test_parse_targeted_metric_pin() {
    let s = parse_suppression(
        7,
        "// qual:allow(srp, file_length=400) reason: \"long but cohesive\"",
    )
    .unwrap();
    assert_eq!(s.dimensions, vec![Dimension::Srp]);
    let target = s.target.expect("should be targeted");
    assert_eq!(target.name, "file_length");
    assert_eq!(target.pin, Some(400.0));
    assert_eq!(s.reason.as_deref(), Some("long but cohesive"));
}

#[test]
fn test_parse_targeted_boolean() {
    let s = parse_suppression(1, "// qual:allow(complexity, unsafe) reason: \"ffi\"").unwrap();
    assert_eq!(s.dimensions, vec![Dimension::Complexity]);
    let target = s.target.expect("should be targeted");
    assert_eq!(target.name, "unsafe");
    assert_eq!(target.pin, None);
}

#[test]
fn test_blanket_allow_has_no_target() {
    let s = parse_suppression(1, "// qual:allow(srp)").unwrap();
    assert!(s.target.is_none());
}

#[test]
fn test_metric_target_requires_value() {
    // A value-less metric suppression would be blind forever — rejected.
    assert!(parse_suppression(1, "// qual:allow(srp, file_length) reason: \"x\"").is_none());
    assert_eq!(
        detect_invalid_qual_allow("// qual:allow(srp, file_length) reason: \"x\""),
        Some(InvalidQualAllow::MetricNeedsValue {
            dim: "srp".into(),
            target: "file_length".into(),
        })
    );
}

#[test]
fn test_boolean_target_rejects_value() {
    assert!(parse_suppression(1, "// qual:allow(complexity, unsafe=5) reason: \"x\"").is_none());
    assert_eq!(
        detect_invalid_qual_allow("// qual:allow(complexity, unsafe=5) reason: \"x\""),
        Some(InvalidQualAllow::BooleanTakesNoValue {
            dim: "complexity".into(),
            target: "unsafe".into(),
        })
    );
}

#[test]
fn test_targeted_requires_reason() {
    // A pin without a reason is rejected — the reason is mandatory friction.
    assert!(parse_suppression(1, "// qual:allow(srp, file_length=400)").is_none());
    assert_eq!(
        detect_invalid_qual_allow("// qual:allow(srp, file_length=400)"),
        Some(InvalidQualAllow::TargetNeedsReason {
            dim: "srp".into(),
            target: "file_length".into(),
        })
    );
}

#[test]
fn test_bad_pin_value() {
    assert_eq!(
        detect_invalid_qual_allow("// qual:allow(srp, file_length=abc) reason: \"x\""),
        Some(InvalidQualAllow::BadPinValue {
            target: "file_length".into(),
            value: "abc".into(),
        })
    );
}

#[test]
fn test_unknown_target_lists_valid_targets() {
    match detect_invalid_qual_allow("// qual:allow(complexity, max_lines=5) reason: \"x\"") {
        Some(InvalidQualAllow::UnknownTarget { dim, target, valid }) => {
            assert_eq!(dim, "complexity");
            assert_eq!(target, "max_lines");
            assert!(valid.contains(&"max_function_lines".to_string()));
        }
        other => panic!("expected UnknownTarget, got {other:?}"),
    }
}
