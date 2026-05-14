use crate::adapters::suppression::qual_allow::*;

#[test]
fn test_parse_qual_allow_bare_is_ignored() {
    // Bare `// qual:allow` (no dimension specified) must be ignored
    // — global suppress doesn't exist as a feature. Authors must
    // spell out the targeted dimension(s) explicitly so typos like
    // `// qual:allow(srp_params)` can't silently hide every finding.
    assert!(parse_suppression(5, "// qual:allow").is_none());
    assert!(parse_suppression(5, "// qual:allow()").is_none());
    assert!(parse_suppression(5, "// qual:allow(srp_params)").is_none());
    assert!(
        parse_suppression(5, "// qual:allow(srp_params) reason: \"typo\"").is_none(),
        "unknown dim + reason still maps to no recognized dim"
    );
}

#[test]
fn test_detect_invalid_qual_allow_flags_typo_dimensions() {
    // Typos like `srp_params` (when the actual dim is `srp`) parse to
    // an empty dimensions list, which would silently do nothing. The
    // detector surfaces them so the orphan-suppression pipeline can
    // emit a stale-marker finding.
    assert_eq!(
        detect_invalid_qual_allow("// qual:allow(srp_params)"),
        Some("srp_params".to_string())
    );
    assert_eq!(
        detect_invalid_qual_allow("// qual:allow(srp_params, foo) reason: \"typo\""),
        Some("srp_params, foo".to_string())
    );
}

#[test]
fn test_detect_invalid_qual_allow_does_not_flag_bare_or_empty() {
    // Bare `// qual:allow` and `// qual:allow()` carry no dim intent
    // — author wrote nothing actionable. Treat as no annotation; the
    // orphan detector should not surface them.
    assert!(detect_invalid_qual_allow("// qual:allow").is_none());
    assert!(detect_invalid_qual_allow("// qual:allow()").is_none());
}

#[test]
fn test_detect_invalid_qual_allow_passes_partial_recognized_dims() {
    // At least one recognized dim → partially valid → not flagged
    // here (the parser keeps the recognized parts as a real
    // Suppression).
    assert!(detect_invalid_qual_allow("// qual:allow(srp, srp_params)").is_none());
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

// ── Test-helper marker tests ─────────────────────────────────

#[test]
fn test_test_helper_marker_exact() {
    assert!(is_test_helper_marker("// qual:test_helper"));
}

#[test]
fn test_test_helper_marker_with_trailing_text() {
    assert!(is_test_helper_marker("// qual:test_helper shared asserter"));
}

#[test]
fn test_test_helper_marker_not_plural() {
    assert!(!is_test_helper_marker("// qual:test_helpers"));
}

#[test]
fn test_test_helper_marker_not_api() {
    assert!(!is_test_helper_marker("// qual:api"));
}

#[test]
fn test_test_helper_marker_not_counted_as_suppression() {
    assert!(parse_suppression(1, "// qual:test_helper").is_none());
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
