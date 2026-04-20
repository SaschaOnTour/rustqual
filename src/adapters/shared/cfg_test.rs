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

/// True if `attrs` contains `#[test]`.
/// Operation: attribute inspection logic, no own calls.
pub fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("test"))
}
