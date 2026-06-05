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
        Some(InvalidQualAllow::UnknownDimensions(
            "srp_params".to_string()
        ))
    );
    assert_eq!(
        detect_invalid_qual_allow("// qual:allow(srp_params, foo) reason: \"typo\""),
        Some(InvalidQualAllow::UnknownDimensions(
            "srp_params, foo".to_string()
        ))
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
fn test_parse_qual_allow_unclosed_parens_is_ignored_as_suppression() {
    // `// qual:allow(iosp` (missing close-paren) used to fall back
    // to `rest.len()` and become a real IOSP suppression. The parser
    // must reject malformed parens entirely so the marker routes
    // through `detect_invalid_qual_allow` instead.
    assert!(parse_suppression(1, "// qual:allow(iosp").is_none());
    assert!(parse_suppression(1, "// qual:allow(srp_params").is_none());
    assert!(parse_suppression(1, "// qual:allow(iosp, complexity").is_none());
}

#[test]
fn test_detect_invalid_qual_allow_catches_unclosed_parens() {
    // Mirror of the parser's reject path: unclosed forms must
    // surface as invalid markers so the typo is visible. Without
    // this, `// qual:allow(srp_params` (no close) would silently
    // disappear — neither suppression nor orphan.
    assert_eq!(
        detect_invalid_qual_allow("// qual:allow(srp_params"),
        Some(InvalidQualAllow::UnclosedParens("srp_params".to_string()))
    );
    assert_eq!(
        detect_invalid_qual_allow("// qual:allow(iosp"),
        Some(InvalidQualAllow::UnclosedParens("iosp".to_string())),
        "unclosed paren is structural malformation — must surface \
         as orphan even when the tail happens to spell a valid dim, \
         otherwise the marker silently drops (no suppression, no \
         orphan) which is exactly the Bug-5 class we closed."
    );
}

#[test]
fn test_invalid_qual_allow_reason_distinguishes_kinds() {
    // Reason-strings rendered into orphan-finding text must
    // accurately describe the failure mode — Copilot caught a
    // version where unclosed-with-valid-dim got the unknown-dim
    // reason ("'iosp' did not parse to any known dimension"),
    // which is outright false (iosp IS a dim; the marker is
    // structurally broken).
    let unknown = InvalidQualAllow::UnknownDimensions("srp_params".to_string()).reason();
    assert!(
        unknown.contains("did not parse to any known dimension"),
        "unknown-dim reason should explain dim resolution failure, got: {unknown}"
    );
    let unclosed = InvalidQualAllow::UnclosedParens("iosp".to_string()).reason();
    assert!(
        unclosed.contains("unclosed parens"),
        "unclosed-paren reason should call out the structural shape, got: {unclosed}"
    );
    assert!(
        !unclosed.contains("did not parse to any known dimension"),
        "unclosed-paren reason must NOT use the unknown-dim wording — \
         it lies for `qual:allow(iosp` (iosp IS a known dim). Got: {unclosed}"
    );
}

#[test]
fn test_detect_invalid_qual_allow_flags_unknown_target() {
    // `srp` is a valid dim, but the second entry is no longer silently
    // dropped — it is read as a *target*, and `srp_params` is not a known
    // srp target, so the marker surfaces as invalid with the valid list.
    // This is exactly what stops an agent inventing a non-existent target.
    match detect_invalid_qual_allow("// qual:allow(srp, srp_params)") {
        Some(InvalidQualAllow::UnknownTarget { dim, target, valid }) => {
            assert_eq!(dim, "srp");
            assert_eq!(target, "srp_params");
            assert!(valid.contains(&"file_length".to_string()));
        }
        other => panic!("expected UnknownTarget, got {other:?}"),
    }
}

#[test]
fn test_parse_qual_allow_iosp() {
    let s = parse_suppression(3, "// qual:allow(iosp)").unwrap();
    assert_eq!(s.dimensions, vec![Dimension::Iosp]);
    assert!(s.reason.is_none());
}

#[test]
fn test_multi_dim_blanket_rejected() {
    // Bare multi-dimension allow is gone: a multi-kind dim must name a target,
    // and one marker cannot target two dimensions. Use separate markers.
    assert!(parse_suppression(1, "// qual:allow(iosp, complexity)").is_none());
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
