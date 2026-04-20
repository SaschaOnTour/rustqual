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
/// Single-pass implementation — avoids the intermediate String
/// allocations of chained `str::replace`.
/// Integration: delegates per-character dispatch to small helpers.
fn normalise_rendering(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        dispatch_char(c, &mut chars, &mut out);
    }
    out
}

/// Emit `c` into `out`, consuming follow-up chars when needed to
/// close up around punctuation.
/// Operation: pattern match + per-kind helper delegation.
fn dispatch_char(c: char, chars: &mut std::iter::Peekable<std::str::Chars<'_>>, out: &mut String) {
    match c {
        ' ' => emit_space(chars, out),
        '<' | '>' => emit_angle(c, chars, out),
        ':' => emit_colon(chars, out),
        other => out.push(other),
    }
}

/// Drop a leading space if the following char is one we close up against.
/// Operation: one-char lookahead with small cloned peek for `::`.
fn emit_space(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, out: &mut String) {
    let keep = match chars.peek() {
        Some('<' | '>' | ',') => false,
        Some(':') => !next_two_are_colons(chars),
        _ => true,
    };
    if keep {
        out.push(' ');
    }
}

/// Emit `<` or `>` and swallow any trailing spaces.
/// Operation: push + consume-while.
fn emit_angle(c: char, chars: &mut std::iter::Peekable<std::str::Chars<'_>>, out: &mut String) {
    out.push(c);
    eat_spaces(chars);
}

/// Emit either `::` (closing up trailing whitespace) or a lone `:`.
/// Operation: one-char lookahead.
fn emit_colon(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, out: &mut String) {
    if chars.peek() == Some(&':') {
        chars.next();
        out.push_str("::");
        eat_spaces(chars);
    } else {
        out.push(':');
    }
}

/// True if the iterator is positioned on `::`. Clones the peekable so
/// the underlying stream is not advanced.
/// Trivial: two peeks via a cloned iterator.
fn next_two_are_colons(chars: &std::iter::Peekable<std::str::Chars<'_>>) -> bool {
    let mut la = chars.clone();
    la.next() == Some(':') && la.next() == Some(':')
}

/// Consume all consecutive spaces from `chars`.
/// Operation: bounded loop, no own calls.
fn eat_spaces(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while chars.peek() == Some(&' ') {
        chars.next();
    }
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
