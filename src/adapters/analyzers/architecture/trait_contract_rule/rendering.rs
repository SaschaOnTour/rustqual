//! Token-stream rendering helpers used by trait-contract checks.
//!
//! These helpers turn `syn` AST fragments back into compact strings so
//! that user-provided substring matchers (`forbidden_return_type_contains`,
//! `required_param_type_contains`, `forbidden_error_variant_contains`)
//! can do their work against a stable rendered form.

use quote::ToTokens;

/// Render a `syn::Type` back to its source form, with spaces stripped.
/// Operation: token-stream stringification.
pub(super) fn render_type(ty: &syn::Type) -> String {
    let mut tokens = proc_macro2::TokenStream::new();
    ty.to_tokens(&mut tokens);
    tokens.to_string().replace(' ', "")
}

/// Render a supertrait bound. Spaces left in so substring matches like
/// `"Send"` don't glue to the next bound.
/// Operation: token stringification.
pub(super) fn render_type_param_bound(bound: &syn::TypeParamBound) -> String {
    let mut tokens = proc_macro2::TokenStream::new();
    bound.to_tokens(&mut tokens);
    tokens.to_string()
}

/// Classify a trait method's receiver as `"shared_ref"`, `"mut_ref"`,
/// or `"owned"`. Returns `None` for methods without a `self` parameter.
/// Operation: pattern-match on FnArg::Receiver.
pub(super) fn receiver_kind(sig: &syn::Signature) -> Option<&'static str> {
    let recv = sig.inputs.iter().find_map(|arg| match arg {
        syn::FnArg::Receiver(r) => Some(r),
        _ => None,
    })?;
    if recv.reference.is_some() {
        if recv.mutability.is_some() {
            Some("mut_ref")
        } else {
            Some("shared_ref")
        }
    } else {
        Some("owned")
    }
}
