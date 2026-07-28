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

use proc_macro2::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};
use syn::parse::Parser;

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
    /// In declaration order, `None` for an arm whose matcher admits no
    /// positions. Kept in place rather than collapsing the list: an unreadable
    /// arm says nothing about an invocation an *earlier* arm already matches.
    rules: Vec<Option<RuleShape>>,
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
    let per_rule: Vec<(Option<RuleShape>, bool)> = rules_of(def)
        .iter()
        .map(|rule| shape_of(rule, targets))
        .collect();
    let applies = per_rule.iter().any(|(_, applies)| *applies);
    // Every rule, in declaration order, including the ones that apply nothing
    // and the ones no position model fits: such a rule can still be the first
    // one that matches, and dropping either kind let a later arm answer for an
    // invocation it never sees.
    let rules = per_rule.into_iter().map(|(shape, _)| shape).collect();
    applies.then_some(CallShape { rules })
}

/// One rule's shape — `None` when its matcher admits no positions — and whether
/// it applies a metavariable at all. The two are independent: a rule with a
/// readable matcher may apply nothing, and a rule that applies something may
/// have a matcher no position model fits.
/// Operation: parameters + called names, own calls in the operands.
fn shape_of(rule: &Rule, targets: &CalledPositions) -> (Option<RuleShape>, bool) {
    let names = called_names(rule, targets);
    let hit = |(i, (name, _)): (usize, &(String, String))| names.contains(name).then_some(i);
    let shape = flat_parameters(&rule.matcher).map(|params| RuleShape {
        fragments: params.iter().map(|(_, frag)| frag.clone()).collect(),
        called: params.iter().enumerate().filter_map(hit).collect(),
    });
    (shape, !names.is_empty())
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
    let called = selected_positions(shape, &args);
    let takes = |i: &usize| called.is_none_or(|set| set.contains(i));
    args.into_iter()
        .enumerate()
        .filter(|(i, _)| takes(i))
        .map(|(_, arg)| arg)
        .collect()
}

/// The positions the first matching rule applies — what `macro_rules!` itself
/// picks. `None` means "take every argument": the first arm that could not be
/// ruled out has no readable matcher, or nothing matched at all.
///
/// Walking in order is the whole point. A readable arm that does not match is
/// stepped over; an unreadable one stops the walk, because nothing here can say
/// it would not have matched — but only once every earlier arm has been ruled
/// out on its own terms.
/// Operation: first decisive rule, own call in the closure.
fn selected_positions<'a>(
    shape: &'a CallShape,
    args: &[TokenStream],
) -> Option<&'a HashSet<usize>> {
    let decides = |rule: &'a Option<RuleShape>| match rule {
        Some(readable) => match accepts_all(readable, args) {
            Match::Accepts => Some(Some(&readable.called)),
            Match::Rejects => None,
            Match::Undecided => Some(None),
        },
        None => Some(None),
    };
    shape.rules.iter().find_map(decides).flatten()
}

/// What a rule says about an argument list.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Match {
    /// Every parameter accepts its argument.
    Accepts,
    /// Some parameter certainly does not — `macro_rules!` moves on.
    Rejects,
    /// Cannot be decided here, so nothing may be concluded from it.
    Undecided,
}

/// What one rule says about these arguments.
///
/// A definite rejection outranks an undecidable parameter: if any argument
/// certainly does not fit, the rule does not match whatever the rest says.
/// Operation: arity check + fold over the parameters, own call in the closure.
fn accepts_all(rule: &RuleShape, args: &[TokenStream]) -> Match {
    if rule.fragments.len() != args.len() {
        return Match::Rejects;
    }
    let verdicts: Vec<Match> = rule
        .fragments
        .iter()
        .zip(args)
        .map(|(frag, arg)| accepts(frag, arg))
        .collect();
    match verdicts {
        v if v.contains(&Match::Rejects) => Match::Rejects,
        v if v.contains(&Match::Undecided) => Match::Undecided,
        _ => Match::Accepts,
    }
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

/// Whether a fragment specifier accepts this argument.
///
/// A kind this cannot check is `Undecided`, **not** a match — treating it as one
/// picks an arm rustc rejects, and if that arm applies nothing, the function the
/// real arm calls is reported dead. That is the expensive direction, and it is
/// the mistake this predicate made until the list below grew.
/// Operation: fragment dispatch, no own calls.
fn accepts(fragment: &str, arg: &TokenStream) -> Match {
    let arg = &without_metavariables(arg);
    let fits = match fragment {
        "ident" => syn::parse2::<syn::Ident>(arg.clone()).is_ok(),
        "path" => syn::parse2::<syn::Path>(arg.clone()).is_ok(),
        "expr" => syn::parse2::<syn::Expr>(arg.clone()).is_ok(),
        "ty" => syn::parse2::<syn::Type>(arg.clone()).is_ok(),
        "literal" => syn::parse2::<syn::Lit>(arg.clone()).is_ok(),
        "block" => syn::parse2::<syn::Block>(arg.clone()).is_ok(),
        "item" => syn::parse2::<syn::Item>(arg.clone()).is_ok(),
        "meta" => syn::parse2::<syn::Meta>(arg.clone()).is_ok(),
        "lifetime" => syn::parse2::<syn::Lifetime>(arg.clone()).is_ok(),
        "pat" => syn::Pat::parse_single.parse2(arg.clone()).is_ok(),
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
