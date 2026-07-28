//! The rule a forwarded fragment follows, as a table rather than as anecdote.
//!
//! Three review rounds in a row found the same defect one layer deeper: a
//! metavariable handed to another macro was judged by how it *looks* — accepted
//! because it contained a `$`, then because substituting an identifier parsed.
//! What decides is the Rust reference:
//!
//! > When forwarding a matched fragment to another macro-by-example, matchers
//! > in the second macro will see an opaque AST of the fragment type. The second
//! > macro can't use literal tokens to match the fragments in the matcher, only
//! > a fragment specifier of the same type. The `ident`, `lifetime`, and `tt`
//! > fragment types are an exception, and can be matched by literal tokens.
//!
//! Enumerating every pair pins that sentence instead of the last example that
//! went wrong. A row that drifts fails here, not three rounds later.

use crate::adapters::shared::macro_params::{forwarded_accepts, Match, TRANSPARENT};

/// Every fragment specifier `macro_rules!` knows.
const KINDS: [&str; 13] = [
    "block", "expr", "ident", "item", "lifetime", "literal", "meta", "pat", "path", "stmt", "tt",
    "ty", "vis",
];

#[test]
fn the_same_kind_always_matches() {
    // The row the whole mechanism rests on: forwarding is what a call-through
    // chain does, and if a kind stopped matching itself every chain would break
    // silently into the coarse fallback.
    for kind in KINDS {
        assert_eq!(
            forwarded_accepts(kind, Some(&kind.to_string())),
            Match::Accepts,
            "{kind} into {kind}"
        );
    }
}

#[test]
fn an_opaque_kind_is_rejected_by_every_other_one() {
    // The direction that cost three rounds: `path` into `ident` looked fine to
    // a syntactic check and is a rejection to rustc. Reading it as a match
    // picks an arm rustc never takes, and where that arm applies nothing, the
    // function the real arm calls is reported dead.
    let opaque: Vec<&str> = KINDS
        .iter()
        .copied()
        .filter(|k| !TRANSPARENT.contains(k))
        .collect();
    for source in &opaque {
        for target in KINDS.iter().filter(|t| *t != source) {
            assert_eq!(
                forwarded_accepts(target, Some(&source.to_string())),
                Match::Rejects,
                "{source} forwarded into {target}"
            );
        }
    }
}

#[test]
fn a_transparent_kind_decides_nothing_on_its_own() {
    // `ident`, `lifetime` and `tt` stay matchable by literal tokens, so the
    // kind alone does not say whether the arm takes them. Undecided, not
    // accepted: claiming a match here is what invents a finding.
    for source in TRANSPARENT {
        for target in KINDS.iter().filter(|t| **t != source) {
            assert_eq!(
                forwarded_accepts(target, Some(&source.to_string())),
                Match::Undecided,
                "{source} forwarded into {target}"
            );
        }
    }
}

#[test]
fn an_unknown_source_decides_nothing() {
    // The enclosing matcher was unreadable, so nothing is known about what the
    // metavariable was captured as.
    for target in KINDS {
        assert_eq!(
            forwarded_accepts(target, None),
            Match::Undecided,
            "{target}"
        );
    }
}

/// A literal argument, the specifier it is offered to, and whether that
/// specifier takes it. Chosen for the rows that decide which arm is selected —
/// a wrong answer here picks an arm rustc never takes.
const LITERAL_CASES: &[(&str, &str, Match)] = &[
    // The case that reported a live function as dead: a bare name is not a
    // block, so the `block` arm is stepped over and the `path` arm calls it.
    ("block", "live_helper", Match::Rejects),
    ("path", "live_helper", Match::Accepts),
    ("ident", "live_helper", Match::Accepts),
    ("expr", "live_helper", Match::Accepts),
    ("literal", "live_helper", Match::Rejects),
    ("lifetime", "live_helper", Match::Rejects),
    ("block", "{ let x = 1; }", Match::Accepts),
    ("expr", "consume(1)", Match::Accepts),
    ("path", "consume(1)", Match::Rejects),
    ("literal", "42", Match::Accepts),
    ("tt", "42", Match::Accepts),
    ("tt", "1 + 2", Match::Rejects),
    ("item", "fn f() {}", Match::Accepts),
    ("stmt", "let x = 1;", Match::Accepts),
    ("lifetime", "'a", Match::Accepts),
    // The one kind left undecided: `vis` matches the empty token stream as
    // readily as a keyword, so a parse says nothing about which arm took it.
    ("vis", "live_helper", Match::Undecided),
    ("vis", "pub", Match::Undecided),
];

#[test]
fn a_literal_argument_is_checked_against_its_specifier() {
    for (fragment, argument, want) in LITERAL_CASES {
        let tokens: proc_macro2::TokenStream = argument.parse().expect("fixture tokenises");
        let got = crate::adapters::shared::macro_params::accepts_literal(fragment, &tokens);
        assert_eq!(got, *want, "{fragment} offered {argument:?}");
    }
}
