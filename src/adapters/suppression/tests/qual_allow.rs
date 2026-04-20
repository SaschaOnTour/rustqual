use crate::adapters::suppression::qual_allow::*;

#[test]
fn test_parse_qual_allow_all() {
    let s = parse_suppression(5, "// qual:allow").unwrap();
    assert_eq!(s.line, 5);
    assert!(s.dimensions.is_empty());
    assert!(s.reason.is_none());
}

#[test]
fn test_parse_qual_allow_iosp() {
    let s = parse_suppression(3, "// qual:allow(iosp)").unwrap();
    assert_eq!(s.dimensions, vec![Dimension::Iosp]);
    assert!(s.reason.is_none());
}

#[test]
fn test_parse_qual_allow_multiple_dims() {
    let s = parse_suppression(1, "// qual:allow(iosp, complexity)").unwrap();
    assert_eq!(s.dimensions, vec![Dimension::Iosp, Dimension::Complexity]);
}

#[test]
fn test_parse_qual_allow_with_reason() {
    let s = parse_suppression(1, "// qual:allow(iosp) reason: \"syn visitor pattern\"").unwrap();
    assert_eq!(s.dimensions, vec![Dimension::Iosp]);
    assert_eq!(s.reason.as_deref(), Some("syn visitor pattern"));
}

#[test]
fn test_parse_old_iosp_allow_still_works() {
    let s = parse_suppression(10, "// iosp:allow").unwrap();
    assert_eq!(s.line, 10);
    assert_eq!(s.dimensions, vec![Dimension::Iosp]);
    assert!(s.reason.is_none());
}

#[test]
fn test_parse_old_iosp_allow_with_reason() {
    let s = parse_suppression(1, "// iosp:allow justified reason").unwrap();
    assert_eq!(s.dimensions, vec![Dimension::Iosp]);
    assert_eq!(s.reason.as_deref(), Some("justified reason"));
}

#[test]
fn test_parse_no_match() {
    assert!(parse_suppression(1, "// normal comment").is_none());
    assert!(parse_suppression(1, "let x = 42;").is_none());
}

// ── API marker tests ─────────────────────────────────────────

#[test]
fn test_api_marker_exact() {
    assert!(is_api_marker("// qual:api"));
}

#[test]
fn test_api_marker_with_trailing_text() {
    assert!(is_api_marker("// qual:api public interface"));
}

#[test]
fn test_api_marker_not_suppression() {
    assert!(!is_api_marker("// qual:allow(dry)"));
}

#[test]
fn test_api_marker_not_regular_comment() {
    assert!(!is_api_marker("// normal comment"));
}

#[test]
fn test_api_marker_not_counted_as_suppression() {
    assert!(parse_suppression(1, "// qual:api").is_none());
}

#[test]
fn test_inverse_marker_parsed() {
    assert_eq!(
        parse_inverse_marker("// qual:inverse(parse)"),
        Some("parse".to_string())
    );
}

#[test]
fn test_inverse_marker_with_spaces() {
    assert_eq!(
        parse_inverse_marker("// qual:inverse( as_str )"),
        Some("as_str".to_string())
    );
}

#[test]
fn test_inverse_marker_empty_rejected() {
    assert_eq!(parse_inverse_marker("// qual:inverse()"), None);
}

#[test]
fn test_inverse_marker_not_suppression() {
    assert!(parse_suppression(1, "// qual:inverse(parse)").is_none());
}
