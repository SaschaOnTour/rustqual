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

/// Which argument positions each known call-through macro applies.
pub(crate) type CalledPositions = HashMap<String, CallShape>;

/// What a macro does with the arguments it is given, rule by rule.
///
/// Per rule, not unioned: `macro_rules!` takes the **first** rule that matches,
/// so two mirrored rules — one applying argument 0, the other argument 1 — say
/// different things about the same invocation, and merging them made every
/// argument look called.
#[derive(Clone, Default)]
pub(crate) struct CallShape {
    /// In declaration order. Empty when no matcher could be read, which is the
    /// coarse case: every argument counts.
    rules: Vec<RuleShape>,
}

/// One rule's parameters and the positions it applies as a callee.
#[derive(Clone)]
struct RuleShape {
    /// The fragment specifier of each parameter (`path`, `expr`, …), in order.
    fragments: Vec<String>,
    called: HashSet<usize>,
}

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
pub(crate) fn called_positions(def: &TokenStream, targets: &CalledPositions) -> Option<CallShape> {
    let rules = rules_of(def);
    let shapes: Vec<Option<RuleShape>> = rules
        .iter()
        .map(|rule| shape_of(rule, targets))
        .filter(|shape| shape.as_ref().is_none_or(|s| !s.called.is_empty()))
        .collect();
    let readable: Option<Vec<RuleShape>> = shapes.iter().cloned().collect();
    (!shapes.is_empty()).then(|| CallShape {
        rules: readable.unwrap_or_default(),
    })
}

/// One rule's shape, or `None` when its matcher does not admit positions.
/// Operation: parameters + called names, own calls in the operands.
fn shape_of(rule: &Rule, targets: &CalledPositions) -> Option<RuleShape> {
    let names = called_names(rule, targets);
    let params = flat_parameters(&rule.matcher)?;
    let hit = |(i, (name, _)): (usize, &(String, String))| names.contains(name).then_some(i);
    Some(RuleShape {
        fragments: params.iter().map(|(_, frag)| frag.clone()).collect(),
        called: params.iter().enumerate().filter_map(hit).collect(),
    })
}

/// The metavariables this rule applies as a callee — directly, or by handing
/// them to a position one of `targets` calls.
/// Operation: two collections merged, own calls in the operands.
fn called_names(rule: &Rule, targets: &CalledPositions) -> HashSet<String> {
    let mut names = called_metavariables(&rule.transcriber);
    names.extend(forwarded_names(&rule.transcriber, targets));
    names
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
        let passed = called.map(|shape| passed_names(&g.stream(), shape));
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
) -> Option<&'a CallShape> {
    let bang = at >= 2 && matches!(&trees[at - 1], TokenTree::Punct(p) if p.as_char() == '!');
    let TokenTree::Ident(name) = trees.get(at.checked_sub(2)?)? else {
        return None;
    };
    bang.then(|| targets.get(&name.to_string())).flatten()
}

/// The arguments this invocation really applies, by the first rule that accepts
/// it. No readable rule yields all of them — the coarse fallback, which
/// suppresses a finding rather than inventing one.
/// Operation: split, select, filter; own calls in the closures.
pub(crate) fn called_arguments(tokens: &TokenStream, shape: &CallShape) -> Vec<TokenStream> {
    let args: Vec<TokenStream> = split_arguments(tokens)
        .into_iter()
        .map(TokenStream::from_iter)
        .collect();
    let called = matching_rule(shape, &args).map(|rule| rule.called.clone());
    let takes = |i: &usize| called.as_ref().is_none_or(|set| set.contains(i));
    args.into_iter()
        .enumerate()
        .filter(|(i, _)| takes(i))
        .map(|(_, arg)| arg)
        .collect()
}

/// The first rule whose parameters accept these arguments — what `macro_rules!`
/// itself picks. `None` when no rule was readable, or none fits.
/// Operation: find over the rules, own call in the closure.
fn matching_rule<'a>(shape: &'a CallShape, args: &[TokenStream]) -> Option<&'a RuleShape> {
    shape.rules.iter().find(|rule| accepts_all(rule, args))
}

/// Whether one rule's parameters accept these arguments.
/// Operation: arity check + per-argument fragment check, own call in the closure.
fn accepts_all(rule: &RuleShape, args: &[TokenStream]) -> bool {
    let fits = |(frag, arg): (&String, &TokenStream)| accepts(frag, arg);
    rule.fragments.len() == args.len() && rule.fragments.iter().zip(args).all(fits)
}

/// Whether a metavariable appears anywhere in these tokens, at any depth —
/// `consume($x)` holds one, and asking only at the top level made the argument
/// look like ordinary code that fails to parse.
/// Operation: recursive token scan, own call in the closure.
// qual:recursive
fn contains_metavariable(tokens: &TokenStream) -> bool {
    tokens.clone().into_iter().any(|tt| match tt {
        TokenTree::Punct(p) => p.as_char() == '$',
        TokenTree::Group(g) => contains_metavariable(&g.stream()),
        _ => false,
    })
}

/// Whether a fragment specifier accepts this argument.
///
/// Only the kinds a token stream can be checked against cheaply; everything else
/// accepts, since failing to disprove a match must not narrow the answer. A
/// metavariable accepts anything: what it stands for was decided one level up.
/// Operation: fragment dispatch, no own calls.
fn accepts(fragment: &str, arg: &TokenStream) -> bool {
    if contains_metavariable(arg) {
        return true;
    }
    match fragment {
        "ident" => syn::parse2::<syn::Ident>(arg.clone()).is_ok(),
        "path" => syn::parse2::<syn::Path>(arg.clone()).is_ok(),
        "expr" => syn::parse2::<syn::Expr>(arg.clone()).is_ok(),
        "ty" => syn::parse2::<syn::Type>(arg.clone()).is_ok(),
        "literal" => syn::parse2::<syn::Lit>(arg.clone()).is_ok(),
        _ => true,
    }
}

/// The metavariable names sitting at a called argument position.
/// Operation: reuse the invocation selection, own calls in the closure.
fn passed_names(tokens: &TokenStream, shape: &CallShape) -> HashSet<String> {
    called_arguments(tokens, shape)
        .iter()
        .flat_map(|arg| metavariables_in(&arg.clone().into_iter().collect::<Vec<_>>()))
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
fn flat_parameters(matcher: &TokenStream) -> Option<Vec<(String, String)>> {
    let trees: Vec<TokenTree> = matcher.clone().into_iter().collect();
    let mut names = Vec::new();
    let mut i = 0;
    while i < trees.len() {
        let dollar = matches!(&trees[i], TokenTree::Punct(p) if p.as_char() == '$');
        let TokenTree::Ident(name) = trees.get(i + 1)? else {
            return None;
        };
        let colon = matches!(trees.get(i + 2), Some(TokenTree::Punct(p)) if p.as_char() == ':');
        let TokenTree::Ident(fragment) = trees.get(i + TRANSCRIBER_OFFSET)? else {
            return None;
        };
        if !(dollar && colon) {
            return None;
        }
        names.push((name.to_string(), fragment.to_string()));
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
