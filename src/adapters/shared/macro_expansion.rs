//! Pre-pass that surfaces test functions hidden inside test-defining
//! macro bodies (`proptest! { … }`, `quickcheck! { … }`).
//!
//! `syn` does not expand function-like macros, so a `#[test] fn` declared
//! inside `proptest! { … }` is an opaque `Item::Macro` — invisible to every
//! analyzer dimension. This module walks the parsed trees once, before any
//! dimension runs, and replaces a recognised test-macro invocation with the
//! `fn` items it declares, each marked `#[test]` (so it routes through the
//! same `cfg_test::has_test_attr` recognition as every other test).
//!
//! It is deliberately lenient: `proptest!`'s `x in <strategy>` parameter
//! grammar is not valid Rust, so the inner functions are reconstructed with
//! an empty parameter list (the body — what length/complexity/DRY measure —
//! is preserved verbatim). Anything that fails to parse is left as the
//! original opaque macro: a blind spot, never a regression.

use syn::parse::{Parse, ParseStream};

use crate::adapters::shared::cfg_test::has_test_attr;

/// Expand recognised test-macro bodies in every parsed file, in place.
/// Integration: delegates the per-file item rewrite.
pub fn expand_test_macros(parsed: &mut [(String, String, syn::File)]) {
    parsed.iter_mut().for_each(|(_, _, file)| {
        let items = std::mem::take(&mut file.items);
        file.items = expand_items(items);
    });
}

/// Rewrite a list of items, expanding test macros and recursing into modules.
/// Integration: flat-maps each item through `expand_item`.
fn expand_items(items: Vec<syn::Item>) -> Vec<syn::Item> {
    items.into_iter().flat_map(expand_item).collect()
}

/// Expand one item: a test macro becomes its `fn`s, a module is recursed
/// into, everything else passes through unchanged.
/// Integration: dispatches on the item kind to a handler.
fn expand_item(item: syn::Item) -> Vec<syn::Item> {
    match item {
        syn::Item::Macro(m) => expand_macro_item(m),
        syn::Item::Mod(m) => vec![syn::Item::Mod(expand_mod(m))],
        other => vec![other],
    }
}

/// Recurse a module's inline body through `expand_items`.
/// Operation: rebinds the module content when present.
fn expand_mod(mut m: syn::ItemMod) -> syn::ItemMod {
    if let Some((brace, items)) = m.content {
        m.content = Some((brace, expand_items(items)));
    }
    m
}

/// Replace a recognised test macro with the `fn`s it declares; otherwise
/// (unrecognised macro or unparseable body) keep the macro item verbatim.
/// Operation: guards on recognition + extraction success.
fn expand_macro_item(m: syn::ItemMacro) -> Vec<syn::Item> {
    if !is_test_macro(&m.mac) {
        return vec![syn::Item::Macro(m)];
    }
    let fns = extract_test_fns(&m.mac);
    if fns.is_empty() {
        vec![syn::Item::Macro(m)]
    } else {
        fns
    }
}

/// True if the macro is a known test-defining macro (`proptest!`,
/// `quickcheck!`), matched on the last path segment so both bare and
/// fully-qualified forms (`proptest::proptest!`) are in scope.
/// Operation: attribute-style path inspection.
fn is_test_macro(mac: &syn::Macro) -> bool {
    mac.path
        .segments
        .last()
        .is_some_and(|seg| seg.ident == "proptest" || seg.ident == "quickcheck")
}

/// Parse the macro body into the `fn` items it declares. Returns an empty
/// vec on any parse failure (the caller then keeps the macro opaque).
/// Operation: delegates to the lenient `MacroTestFns` parser.
fn extract_test_fns(mac: &syn::Macro) -> Vec<syn::Item> {
    syn::parse2::<MacroTestFns>(mac.tokens.clone())
        .map(|parsed| parsed.fns)
        .unwrap_or_default()
}

/// Lenient parser for the body of a test-defining macro: a leading run of
/// inner attributes (e.g. `#![proptest_config(…)]`) followed by one or more
/// `[#attrs] [vis] fn name[<generics>](<ignored params>) [-> ret] [where …]
/// { block }` shapes.
struct MacroTestFns {
    fns: Vec<syn::Item>,
}

impl Parse for MacroTestFns {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Drain a leading inner-attribute block such as proptest's
        // `#![proptest_config(Config { .. })]` — it is config, not a fn.
        let _config = input.call(syn::Attribute::parse_inner)?;
        let mut fns = Vec::new();
        while !input.is_empty() {
            fns.push(parse_one_test_fn(input)?);
        }
        Ok(Self { fns })
    }
}

/// Parse a single inner function shape and rebuild it as a `#[test] fn`
/// with an empty parameter list. The original parameters (which may use
/// `proptest`'s non-Rust `x in <strategy>` grammar) are intentionally
/// dropped; the body, attributes, generics and return type are preserved.
/// Operation: sequential token parsing + struct construction.
fn parse_one_test_fn(input: ParseStream) -> syn::Result<syn::Item> {
    let mut attrs = input.call(syn::Attribute::parse_outer)?;
    let _vis: syn::Visibility = input.parse()?;
    let fn_token: syn::Token![fn] = input.parse()?;
    let ident: syn::Ident = input.parse()?;
    let mut generics: syn::Generics = input.parse()?;
    let params;
    syn::parenthesized!(params in input);
    let _ignored: proc_macro2::TokenStream = params.parse()?;
    let output: syn::ReturnType = input.parse()?;
    generics.where_clause = input.parse()?;
    let block: syn::Block = input.parse()?;

    if !has_test_attr(&attrs) {
        attrs.push(syn::parse_quote!(#[test]));
    }
    let sig = syn::Signature {
        constness: None,
        asyncness: None,
        unsafety: None,
        abi: None,
        fn_token,
        ident,
        generics,
        paren_token: syn::token::Paren::default(),
        inputs: syn::punctuated::Punctuated::new(),
        variadic: None,
        output,
    };
    Ok(syn::Item::Fn(syn::ItemFn {
        attrs,
        vis: syn::Visibility::Inherited,
        sig,
        block: Box::new(block),
    }))
}
