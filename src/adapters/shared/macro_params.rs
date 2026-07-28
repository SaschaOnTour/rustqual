//! Which argument of a `macro_rules!` macro ends up being *called*.
//!
//! A macro that applies a metavariable — `($f:path, $label:expr) => { $f(); }` —
//! calls whatever the invocation passes at `$f`'s position and merely uses the
//! rest. Deciding "this invocation forwards a callee" from *any* `$` in the
//! argument list therefore excuses too much: `step!(live_helper, consume($x))`
//! never calls `$x`.
//!
//! The model holds for a **flat, comma-separated matcher**, which is what the
//! shape needs to be readable at all. A repetition (`$($t:path),*`), a custom
//! separator or literal tokens in the matcher put the position out of reach of a
//! token scan, and the answer is then `None` — the caller falls back to the
//! coarse "any metavariable" rule, which over-approximates in the direction that
//! suppresses a finding rather than inventing one.

use std::collections::HashSet;

use proc_macro2::{Delimiter, TokenStream, TokenTree};

use super::macro_tokens::{called_metavariables, is_quoted_at};

/// `(matcher) => {transcriber}`: the body sits three trees past the matcher —
/// the `=`, the `>`, then the group.
const TRANSCRIBER_OFFSET: usize = 3;

/// `$name : fragment` — four trees per matcher parameter.
const PARAMETER_TOKENS: usize = 4;

/// One `matcher => transcriber` rule of a definition.
struct Rule {
    matcher: TokenStream,
    transcriber: TokenStream,
}

/// The argument positions this definition applies as a callee, or `None` when
/// its matcher is not a flat comma-separated parameter list.
/// Integration: split into rules, then fold each rule's positions.
pub(crate) fn called_argument_positions(tokens: &TokenStream) -> Option<HashSet<usize>> {
    let rules = rules_of(tokens);
    let per_rule: Option<Vec<HashSet<usize>>> = rules.iter().map(positions_in).collect();
    per_rule.map(|sets| sets.into_iter().flatten().collect())
}

/// The positions one rule calls. `None` when its matcher is not flat.
/// Operation: parameter list + called names, own calls in the operands.
fn positions_in(rule: &Rule) -> Option<HashSet<usize>> {
    let params = flat_parameters(&rule.matcher)?;
    let called = called_metavariables(&rule.transcriber);
    let hit = |(i, p): (usize, &String)| called.contains(p).then_some(i);
    Some(params.iter().enumerate().filter_map(hit).collect())
}

/// Every `(…) => {…}` pair in a definition body.
/// Operation: positional scan over the token trees, no own calls.
fn rules_of(tokens: &TokenStream) -> Vec<Rule> {
    let trees: Vec<TokenTree> = tokens.clone().into_iter().collect();
    let mut out = Vec::new();
    for (i, tt) in trees.iter().enumerate() {
        let TokenTree::Group(matcher) = tt else {
            continue;
        };
        let arrow = matches!(trees.get(i + 1), Some(TokenTree::Punct(p)) if p.as_char() == '=')
            && matches!(trees.get(i + 2), Some(TokenTree::Punct(p)) if p.as_char() == '>');
        let body = match trees.get(i + TRANSCRIBER_OFFSET) {
            Some(TokenTree::Group(g)) if arrow => Some(g.stream()),
            _ => None,
        };
        out.extend(body.map(|transcriber| Rule {
            matcher: matcher.stream(),
            transcriber,
        }));
    }
    out
}

/// The metavariable names of a flat `$a:frag, $b:frag` matcher, in order.
/// `None` for anything else — a repetition, a separator that is not a comma, a
/// literal token — where an argument's position says nothing.
/// Operation: state-free scan in groups of four, no own calls.
fn flat_parameters(matcher: &TokenStream) -> Option<Vec<String>> {
    let trees: Vec<TokenTree> = matcher.clone().into_iter().collect();
    let mut names = Vec::new();
    let mut i = 0;
    while i < trees.len() {
        let dollar = matches!(&trees[i], TokenTree::Punct(p) if p.as_char() == '$');
        let TokenTree::Ident(name) = trees.get(i + 1)? else {
            return None;
        };
        let colon = matches!(trees.get(i + 2), Some(TokenTree::Punct(p)) if p.as_char() == ':');
        let fragment = matches!(trees.get(i + 3), Some(TokenTree::Ident(_)));
        if !(dollar && colon && fragment) {
            return None;
        }
        names.push(name.to_string());
        i += PARAMETER_TOKENS;
        match trees.get(i) {
            None => return Some(names),
            Some(TokenTree::Punct(p)) if p.as_char() == ',' => i += 1,
            Some(_) => return None,
        }
    }
    Some(names)
}

/// The invocation's top-level arguments, split on commas.
/// Operation: fold over the token trees, no own calls.
pub(crate) fn split_arguments(tokens: &TokenStream) -> Vec<Vec<TokenTree>> {
    let mut args: Vec<Vec<TokenTree>> = vec![Vec::new()];
    tokens.clone().into_iter().for_each(|tt| {
        let comma = matches!(&tt, TokenTree::Punct(p) if p.as_char() == ',');
        match comma {
            true => args.push(Vec::new()),
            false => args.last_mut().unwrap_or(&mut Vec::new()).push(tt),
        }
    });
    args
}

/// Whether these tokens pass a metavariable on rather than quoting one.
/// Operation: positional scan, own call in the closure.
pub(crate) fn hands_over(trees: &[TokenTree]) -> bool {
    trees.iter().enumerate().any(|(i, tt)| match tt {
        TokenTree::Punct(p) => p.as_char() == '$',
        TokenTree::Group(g) if !is_quoted_at(trees, i) => {
            hands_over(&g.stream().into_iter().collect::<Vec<_>>())
        }
        _ => false,
    })
}

/// Whether a group delimited by parentheses holds the invocation's arguments.
/// Operation: delimiter check, no own calls.
pub(crate) fn is_argument_list(delimiter: Delimiter) -> bool {
    matches!(delimiter, Delimiter::Parenthesis | Delimiter::Bracket)
}
