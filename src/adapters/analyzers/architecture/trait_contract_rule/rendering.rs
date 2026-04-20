//! Token-stream rendering helpers used by trait-contract checks.
//!
//! These helpers turn `syn` AST fragments back into compact strings so
//! that user-provided substring matchers (`forbidden_return_type_contains`,
//! `required_param_type_contains`, `forbidden_error_variant_contains`)
//! can do their work against a stable rendered form.

use quote::ToTokens;

/// Render a `syn::Type` back to source-like form for substring matching.
///
/// `proc_macro2::TokenStream::to_string()` inserts a single space between
/// every token, so `Box<dyn Error + Send>` becomes `"Box < dyn Error + Send >"`.
/// We collapse the whitespace around `::`, `<`, `>` completely, and the
/// whitespace *before* `,` — the space *after* a comma is preserved so
/// the rendered form matches how users write generic arguments
/// (`Result<T, E>`, not `Result<T,E>`). Spaces between identifiers
/// stay intact so keyword patterns like `"dyn Error"` or `"impl Trait"`
/// match as written.
/// Operation: token-stream stringification + targeted whitespace normalisation.
pub(super) fn render_type(ty: &syn::Type) -> String {
    let mut tokens = proc_macro2::TokenStream::new();
    ty.to_tokens(&mut tokens);
    normalise_rendering(&tokens.to_string())
}

/// Collapse whitespace that `TokenStream::to_string()` inserts around
/// punctuation. `::`, `<`, `>` are fully closed up; `,` keeps its
/// trailing space for readability (matches common source form).
/// Operation: sequential string replacements.
fn normalise_rendering(s: &str) -> String {
    s.replace(" :: ", "::")
        .replace(":: ", "::")
        .replace(" ::", "::")
        .replace(" < ", "<")
        .replace("< ", "<")
        .replace(" <", "<")
        .replace(" > ", ">")
        .replace(" >", ">")
        .replace("> ", ">")
        .replace(" ,", ",")
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
