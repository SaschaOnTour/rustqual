//! Shared attribute helpers for recognising test code.
//!
//! Every analyzer that distinguishes test code from production code needs
//! the same pair of predicates over `syn::Attribute`. Hosting them here
//! keeps the rule simple (no cross-analyzer imports) and the semantics
//! uniform across IOSP, DRY, TQ, Structural, and Architecture.

/// True if `attrs` contains `#[cfg(test)]`.
/// Operation: attribute inspection logic, no own calls.
pub fn has_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Ident>()
                .is_ok_and(|ident| ident == "test")
    })
}

/// True if `attrs` contains a test-entry-point attribute.
///
/// Recognises the bare `#[test]` plus the common framework variants:
/// any attribute whose path ends in `test` (`#[tokio::test]`,
/// `#[async_std::test]`, `#[googletest::test]`, …) and the renamed
/// macros `#[rstest]`, `#[test_case]` and `#[quickcheck]`. Matching on
/// the last path segment keeps both bare and fully-qualified forms
/// (`#[rstest::rstest]`) in scope without an exhaustive crate list.
/// Operation: attribute inspection logic, no own calls.
pub fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().segments.last().is_some_and(|seg| {
            seg.ident == "test"
                || seg.ident == "rstest"
                || seg.ident == "test_case"
                || seg.ident == "quickcheck"
        })
    })
}
