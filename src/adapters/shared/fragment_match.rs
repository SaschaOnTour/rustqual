//! Whether a macro's fragment specifier takes a given argument.
//!
//! `macro_rules!` picks the first rule whose matcher accepts the invocation, so
//! deciding which arm runs means deciding, per parameter, whether its specifier
//! takes the argument in front of it. Two rules govern that, and neither is
//! syntactic:
//!
//! - A **forwarded** metavariable keeps the fragment it was captured as, and
//!   rustc keeps it opaque: only a specifier of the same fragment consumes it.
//!   `ident`, `lifetime` and `tt` are the documented exceptions, matched like
//!   ordinary tokens.
//! - Everything else is decided by parsing, with any metavariable *inside* the
//!   argument substituted first — `consume($x)` is a call expression whatever
//!   `$x` holds.
//!
//! Answering "accepts" too readily picks an arm rustc never takes; where that
//! arm applies nothing, the function the real arm calls is reported dead. Every
//! table here is therefore checked against the compiler rather than remembered
//! — which is how `expr_2021` and `pat_param`, and the fact that they accept
//! `expr` and `pat`, came to light.

use std::collections::HashMap;

use proc_macro2::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};
use syn::parse::Parser;

/// The fragment each metavariable of the *enclosing* rule was captured as.
/// Empty at a real invocation, where no metavariable can appear.
pub(crate) type Fragments = HashMap<String, String>;

/// Fragments that stay transparent when forwarded: rustc lets these be matched
/// literally or by another specifier, unlike the opaque rest.
pub(crate) const TRANSPARENT: [&str; 3] = ["ident", "lifetime", "tt"];

/// Fragment names that are the same fragment under an edition's spelling. A
/// matcher of either takes the other when forwarded — verified against rustc,
/// which is also how the pair was found: comparing names alone called
/// `expr_2021` and `expr` incompatible.
const EDITION_VARIANTS: [[&str; 2]; 2] = [["expr", "expr_2021"], ["pat", "pat_param"]];

/// Every fragment specifier this analysis knows. A name outside this list is a
/// specifier a newer compiler added, and nothing here may claim to know what it
/// accepts — see `forwarded_accepts`, which answers `Undecided` for it.
///
/// Pinned against the compiler's own list by
/// `shared/tests/fragment_match.rs::the_known_specifiers_match_the_compiler`, so
/// a toolchain that adds one says so instead of the analysis quietly guessing.
pub(crate) const KNOWN_FRAGMENTS: [&str; 15] = [
    "block",
    "expr",
    "expr_2021",
    "ident",
    "item",
    "lifetime",
    "literal",
    "meta",
    "pat",
    "pat_param",
    "path",
    "stmt",
    "tt",
    "ty",
    "vis",
];

/// What a rule says about an argument list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Match {
    /// Every parameter accepts its argument.
    Accepts,
    /// Some parameter certainly does not — `macro_rules!` moves on.
    Rejects,
    /// Cannot be decided here, so nothing may be concluded from it.
    Undecided,
}

/// Whether a fragment specifier accepts this argument.
///
/// A kind this cannot check is `Undecided`, **not** a match — treating it as one
/// picks an arm rustc rejects, and if that arm applies nothing, the function the
/// real arm calls is reported dead. That is the expensive direction, and it is
/// the mistake this predicate made until the list below grew.
/// Operation: fragment dispatch, no own calls.
pub(crate) fn accepts(fragment: &str, arg: &TokenStream, enclosing: &Fragments) -> Match {
    match bare_metavariable(arg) {
        Some(name) => forwarded_accepts(fragment, enclosing.get(&name)),
        None => accepts_literal(fragment, arg),
    }
}

/// Whether a specifier takes an argument made of ordinary tokens. A
/// metavariable *inside* it is substituted first, since there the surrounding
/// shape decides — `consume($x)` is a call expression whatever `$x` holds.
/// Operation: fragment dispatch, no own calls.
pub(crate) fn accepts_literal(fragment: &str, arg: &TokenStream) -> Match {
    let arg = &without_metavariables(arg);
    let fits = match fragment {
        "ident" => syn::parse2::<syn::Ident>(arg.clone()).is_ok(),
        "path" => syn::parse2::<syn::Path>(arg.clone()).is_ok(),
        "expr" => return expr_accepts(arg, EditionSpan::Any),
        "expr_2021" => return expr_accepts(arg, EditionSpan::Before2024),
        "ty" => syn::parse2::<syn::Type>(arg.clone()).is_ok(),
        "literal" => syn::parse2::<syn::Lit>(arg.clone()).is_ok(),
        "block" => syn::parse2::<syn::Block>(arg.clone()).is_ok(),
        "item" => syn::parse2::<syn::Item>(arg.clone()).is_ok(),
        "meta" => syn::parse2::<syn::Meta>(arg.clone()).is_ok(),
        "lifetime" => syn::parse2::<syn::Lifetime>(arg.clone()).is_ok(),
        "pat" => return pat_accepts(arg),
        // The shape that never took a top-level `|`, in any edition.
        "pat_param" => syn::Pat::parse_single.parse2(arg.clone()).is_ok(),
        "stmt" => is_single_statement(arg),
        "tt" => arg.clone().into_iter().count() == 1,
        // `vis` matches the empty token stream as readily as a keyword, so a
        // parse says nothing about whether this argument is the one it took.
        _ => return Match::Undecided,
    };
    match fits {
        true => Match::Accepts,
        false => Match::Rejects,
    }
}

/// Whether these tokens are exactly one statement.
/// Operation: brace-wrap + parse, no own calls.
fn is_single_statement(arg: &TokenStream) -> bool {
    let braced = TokenStream::from(TokenTree::Group(Group::new(Delimiter::Brace, arg.clone())));
    syn::parse2::<syn::Block>(braced).is_ok_and(|block| block.stmts.len() == 1)
}

/// Whether a matcher of kind `fragment` takes a forwarded metavariable captured
/// as `source`.
///
/// rustc keeps a matched fragment **opaque**: only a specifier of the same kind
/// consumes it, so a forwarded `path` is not an `ident` however much it looks
/// like one. `ident`, `lifetime` and `tt` are the documented exceptions — they
/// stay transparent and are matched like ordinary tokens. An unknown source
/// (the enclosing matcher was unreadable) decides nothing.
/// Operation: two comparisons, no own calls.
pub(crate) fn forwarded_accepts(fragment: &str, source: Option<&String>) -> Match {
    let Some(source) = source else {
        return Match::Undecided;
    };
    match (source.as_str(), fragment) {
        (from, to) if from == to || same_fragment(from, to) => Match::Accepts,
        (from, _) if TRANSPARENT.contains(&from) => Match::Undecided,
        // A name this does not know is one a newer compiler added, and its
        // compatibilities are unknown with it — `expr_2021` was exactly that,
        // and reading an unknown name as "rejects" is how a live function gets
        // reported dead. Rejection is claimed only between known kinds.
        (from, to) if !known(from) || !known(to) => Match::Undecided,
        _ => Match::Rejects,
    }
}

/// Whether two specifier names denote one fragment.
/// Operation: table lookup, own calls in the closure.
fn same_fragment(a: &str, b: &str) -> bool {
    EDITION_VARIANTS
        .iter()
        .any(|pair| pair.contains(&a) && pair.contains(&b))
}

/// Whether this specifier is one the analysis has been taught.
/// Operation: table lookup, no own calls.
fn known(fragment: &str) -> bool {
    KNOWN_FRAGMENTS.contains(&fragment)
}

/// The same tokens with every `$name` replaced by a plain identifier.
///
/// A forwarded metavariable is not a wildcard: `$f:path` handed to
/// `choose!($f)` still carries `path`, and a `$body:block` arm does not take
/// it. Accepting any argument that holds a metavariable therefore picked arms
/// rustc rejects — and where such an arm applies nothing, the function the real
/// arm calls was reported dead.
///
/// Substituting keeps the argument's *shape* — `consume($x)` stays a call
/// expression, a bare `$f` stays a single name — so the ordinary fragment
/// parsers answer, and they answer the way rustc does for the cases that matter.
/// A `$` that names nothing (the head of a repetition) is dropped rather than
/// left to fail the parse.
/// Operation: token rewrite, own call in the closure.
// qual:recursive
fn without_metavariables(tokens: &TokenStream) -> TokenStream {
    let trees: Vec<TokenTree> = tokens.clone().into_iter().collect();
    let mut out: Vec<TokenTree> = Vec::new();
    let mut i = 0;
    while i < trees.len() {
        let dollar = matches!(&trees[i], TokenTree::Punct(p) if p.as_char() == '$');
        let named = matches!(trees.get(i + 1), Some(TokenTree::Ident(_)));
        match (&trees[i], dollar, named) {
            (_, true, true) => {
                out.push(TokenTree::Ident(Ident::new(SUBSTITUTE, Span::call_site())));
                i += 1;
            }
            (_, true, false) => {}
            (TokenTree::Group(g), ..) => out.push(TokenTree::Group(Group::new(
                g.delimiter(),
                without_metavariables(&g.stream()),
            ))),
            (other, ..) => out.push(other.clone()),
        }
        i += 1;
    }
    out.into_iter().collect()
}

/// The identifier a metavariable is replaced by. Any name would do; this one
/// cannot collide with a real one.
const SUBSTITUTE: &str = "__rustqual_fragment";

/// The single metavariable this argument consists of, if that is all it is.
/// Operation: shape check over two trees, no own calls.
fn bare_metavariable(arg: &TokenStream) -> Option<String> {
    let trees: Vec<TokenTree> = arg.clone().into_iter().collect();
    let dollar = matches!(trees.first(), Some(TokenTree::Punct(p)) if p.as_char() == '$');
    match (trees.len(), dollar, trees.get(1)) {
        (2, true, Some(TokenTree::Ident(name))) => Some(name.to_string()),
        _ => None,
    }
}

/// Which editions a specifier's *name* covers.
enum EditionSpan {
    /// `expr` — its meaning moved with the editions.
    Any,
    /// `expr_2021` — the frozen shape, whatever the crate's edition.
    Before2024,
}

/// Whether an expression specifier takes this argument.
///
/// `const { … }` and `_` became expressions for `expr` in edition 2024 and are
/// not `expr_2021` in any edition. rustqual reads no `Cargo.toml` and so knows
/// no edition, which is exactly why `expr` must answer `Undecided` for those
/// two rather than guess: guessing "accepts" picks an arm a 2021 crate never
/// takes, guessing "rejects" picks one a 2024 crate never takes, and one of
/// those directions reports a live function as dead.
/// Operation: parse + variant check, no own calls.
fn expr_accepts(arg: &TokenStream, span: EditionSpan) -> Match {
    let Ok(expr) = syn::parse2::<syn::Expr>(arg.clone()) else {
        return Match::Rejects;
    };
    let since_2024 = matches!(expr, syn::Expr::Const(_) | syn::Expr::Infer(_));
    match (span, since_2024) {
        (_, false) => Match::Accepts,
        (EditionSpan::Before2024, true) => Match::Rejects,
        (EditionSpan::Any, true) => Match::Undecided,
    }
}

/// Whether `pat` takes this argument.
///
/// A top-level or-pattern is a `pat` from edition 2021 on and was not before,
/// so an argument that needs one is edition-dependent and stays `Undecided`.
/// Anything that parses without the leading alternation is a `pat` in every
/// edition.
/// Operation: two parses, no own calls.
fn pat_accepts(arg: &TokenStream) -> Match {
    if syn::Pat::parse_single.parse2(arg.clone()).is_ok() {
        return Match::Accepts;
    }
    match syn::Pat::parse_multi_with_leading_vert.parse2(arg.clone()) {
        Ok(_) => Match::Undecided,
        Err(_) => Match::Rejects,
    }
}
