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

use std::collections::{HashMap, HashSet};

use proc_macro2::{TokenStream, TokenTree};

use super::macro_tokens::{called_metavariables, is_quoted_at};

/// Which argument positions each known call-through macro applies. `None` for a
/// macro whose matcher is not flat, where the caller accepts a metavariable at
/// any position.
pub(crate) type CalledPositions = HashMap<String, Option<HashSet<usize>>>;

/// One `matcher => transcriber` rule of a definition.
struct Rule {
    matcher: TokenStream,
    transcriber: TokenStream,
}

/// `(matcher) => {transcriber}`: the body sits three trees past the matcher —
/// the `=`, the `>`, then the group.
const TRANSCRIBER_OFFSET: usize = 3;

/// `$name : fragment` — four trees per matcher parameter.
const PARAMETER_TOKENS: usize = 4;

/// The argument positions this definition applies as a callee, given what is
/// already known about the macros it invokes. `None` when it calls nothing
/// through; `Some(None)` when it does but its matcher does not admit positions.
///
/// The recursion is in the caller's fixpoint, not here: a macro that forwards to
/// a macro that forwards keeps its own position, because the target's positions
/// are read from `targets` and mapped back onto this matcher.
/// Integration: rules, names, then the positions those names sit at.
pub(crate) fn called_positions(
    def: &TokenStream,
    targets: &CalledPositions,
) -> Option<Option<HashSet<usize>>> {
    let rules = rules_of(def);
    let named: Vec<(&Rule, HashSet<String>)> = rules
        .iter()
        .map(|rule| (rule, called_names(rule, targets)))
        .filter(|(_, names)| !names.is_empty())
        .collect();
    let positions: Option<HashSet<usize>> = named
        .iter()
        .map(|(rule, names)| positions_of(&rule.matcher, names))
        .try_fold(HashSet::new(), |acc, set| Some(union(acc, set?)));
    (!named.is_empty()).then_some(positions)
}

/// Operation: set union, no own calls.
fn union(mut acc: HashSet<usize>, other: HashSet<usize>) -> HashSet<usize> {
    acc.extend(other);
    acc
}

/// The metavariables this rule applies as a callee — directly, or by handing
/// them to a position one of `targets` calls.
/// Operation: two collections merged, own calls in the operands.
fn called_names(rule: &Rule, targets: &CalledPositions) -> HashSet<String> {
    let mut names = called_metavariables(&rule.transcriber);
    names.extend(forwarded_names(&rule.transcriber, targets));
    names
}

/// Where `names` sit in a flat matcher. `None` for any other matcher shape,
/// where a position says nothing.
/// Operation: parameter list + index filter, own call in the operand.
fn positions_of(matcher: &TokenStream, names: &HashSet<String>) -> Option<HashSet<usize>> {
    let params = flat_parameters(matcher)?;
    let hit = |(i, p): (usize, &String)| names.contains(p).then_some(i);
    Some(params.iter().enumerate().filter_map(hit).collect())
}

/// The metavariables this transcriber hands to a position one of `targets`
/// really calls. `stringify!($x)` hands over nothing, and neither does an
/// argument at a position the target only reads.
/// Operation: positional scan, own calls in the closures.
// qual:recursive
fn forwarded_names(tokens: &TokenStream, targets: &CalledPositions) -> HashSet<String> {
    let trees: Vec<TokenTree> = tokens.clone().into_iter().collect();
    let mut out = HashSet::new();
    for (i, tt) in trees.iter().enumerate() {
        let TokenTree::Group(g) = tt else { continue };
        if is_quoted_at(&trees, i) {
            continue;
        }
        out.extend(forwarded_names(&g.stream(), targets));
        let called = invoked_target(&trees, i, targets);
        let passed = called.map(|positions| passed_names(&g.stream(), positions));
        out.extend(passed.unwrap_or_default());
    }
    out
}

/// The positions the macro invoked just before tree `at` calls, if it is known.
/// Operation: two-token lookback + map lookup, no own calls.
fn invoked_target<'a>(
    trees: &[TokenTree],
    at: usize,
    targets: &'a CalledPositions,
) -> Option<&'a Option<HashSet<usize>>> {
    let bang = at >= 2 && matches!(&trees[at - 1], TokenTree::Punct(p) if p.as_char() == '!');
    let TokenTree::Ident(name) = trees.get(at.checked_sub(2)?)? else {
        return None;
    };
    bang.then(|| targets.get(&name.to_string())).flatten()
}

/// The arguments a macro with these `positions` actually applies. An unreadable
/// matcher (`None`) yields all of them — the coarse fallback, which suppresses a
/// finding rather than inventing one.
/// Operation: split + position filter, own calls in the closures.
pub(crate) fn called_arguments(
    tokens: &TokenStream,
    positions: &Option<HashSet<usize>>,
) -> Vec<TokenStream> {
    let called = |i: &usize| positions.as_ref().is_none_or(|set| set.contains(i));
    split_arguments(tokens)
        .into_iter()
        .enumerate()
        .filter(|(i, _)| called(i))
        .map(|(_, arg)| TokenStream::from_iter(arg))
        .collect()
}

/// The metavariable names sitting at a called argument position.
/// Operation: split + position filter, own calls in the closures.
fn passed_names(tokens: &TokenStream, positions: &Option<HashSet<usize>>) -> HashSet<String> {
    let called = |i: &usize| positions.as_ref().is_none_or(|set| set.contains(i));
    split_arguments(tokens)
        .iter()
        .enumerate()
        .filter(|(i, _)| called(i))
        .flat_map(|(_, arg)| metavariables_in(arg))
        .collect()
}

/// Every `$name` in a token slice, skipping what a quoting macro holds.
/// Operation: positional scan, own call in the closure.
// qual:recursive
fn metavariables_in(trees: &[TokenTree]) -> HashSet<String> {
    let mut out = HashSet::new();
    for (i, tt) in trees.iter().enumerate() {
        let dollar = matches!(tt, TokenTree::Punct(p) if p.as_char() == '$');
        if let (true, Some(TokenTree::Ident(name))) = (dollar, trees.get(i + 1)) {
            out.insert(name.to_string());
        }
        match tt {
            TokenTree::Group(g) if !is_quoted_at(trees, i) => {
                out.extend(metavariables_in(
                    &g.stream().into_iter().collect::<Vec<_>>(),
                ));
            }
            _ => {}
        }
    }
    out
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
        let fragment = matches!(trees.get(i + TRANSCRIBER_OFFSET), Some(TokenTree::Ident(_)));
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
