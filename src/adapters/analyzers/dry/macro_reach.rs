//! What invoking a `macro_rules!` macro reaches.
//!
//! rustqual does not expand macros — that means rustc's matcher, fragment
//! types, repetitions and hygiene, and for a proc macro it means building the
//! crate. What the call graph needs here is far weaker: a test that invokes
//! `run_suite!(store)` really does run whatever the macro's *definition* names,
//! so those names count as test-reached.
//!
//! Deliberately coarse. A macro that names many functions marks all of them
//! reached, even the ones a particular invocation does not touch. That is
//! acceptable exactly once: the set it feeds — test-reached names — only ever
//! *suppresses* findings, so over-collecting costs a missed finding and never
//! invents one. It is not used for production calls, where the same generosity
//! would hide dead code.

use std::collections::{HashMap, HashSet};

use proc_macro2::TokenStream;

use crate::adapters::shared::file_visitor::{visit_all_files, FileVisitor};
use crate::adapters::shared::{macro_params, macro_tokens};

/// Macro name → every name its body mentions, following nested macro
/// invocations to their own bodies.
pub(crate) type MacroReach = HashMap<String, Vec<String>>;

/// Every `macro_rules!` body in the workspace, keyed by name.
///
/// One walk for both derivations below: what a macro *reaches* and which
/// arguments it *applies* are two questions about the same bodies, and
/// collecting them twice meant a second traversal that grows with the repo.
/// Operation: one visit, no own calls beyond the driver.
pub(crate) fn collect_macro_bodies(
    parsed: &[(String, String, syn::File)],
) -> HashMap<String, TokenStream> {
    let mut collector = MacroBodyCollector::default();
    visit_all_files(parsed, &mut collector);
    collector.bodies
}

/// What each `macro_rules!` macro in the workspace reaches.
/// Integration: per-body idents, then the transitive closure.
pub(crate) fn macro_reach_of(bodies: &HashMap<String, TokenStream>) -> MacroReach {
    let named = bodies
        .iter()
        .map(|(name, body)| (name.clone(), macro_tokens::all_idents(body).collect()));
    close_over_nested(named.collect())
}

/// Follow a macro that invokes another macro to what that one reaches. Bounded
/// by the number of macros, so a cyclic pair cannot loop.
/// Operation: fixpoint over the definition map, no own calls.
fn close_over_nested(bodies: MacroReach) -> MacroReach {
    let mut out: MacroReach = bodies.clone();
    for _ in 0..bodies.len() {
        let mut changed = false;
        for (name, direct) in &bodies {
            let mut reached: HashSet<String> = out
                .get(name)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();
            let before = reached.len();
            // Against `out`, not `bodies`: reading the unexpanded map would
            // follow exactly one level, so `a! -> b! -> c! -> helper` stops at
            // `c` however often the loop runs.
            let nested: Vec<String> = direct
                .iter()
                .filter_map(|n| out.get(n))
                .flat_map(|names| names.iter().cloned())
                .collect();
            reached.extend(nested);
            changed |= reached.len() != before;
            out.insert(name.clone(), reached.into_iter().collect());
        }
        if !changed {
            break;
        }
    }
    out
}

/// Visitor recording each `macro_rules!` definition body, keyed by its name.
#[derive(Default)]
struct MacroBodyCollector {
    bodies: HashMap<String, TokenStream>,
}

impl FileVisitor for MacroBodyCollector {
    fn reset_for_file(&mut self, _file_path: &str) {}
}

impl<'ast> syn::visit::Visit<'ast> for MacroBodyCollector {
    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        let Some(name) = node.ident.as_ref().map(|i| i.to_string()) else {
            return;
        };
        self.bodies.insert(name, node.mac.tokens.clone());
    }
}

/// Macros that call *through* a metavariable: their body contains `$name(…)`,
/// or they hand a metavariable to a macro that does.
///
/// `run_suite!(make; check_append, check_rotate)` names the functions it runs
/// as bare idents — an ident followed by a comma is not in call position, so
/// the token walk sees nothing and the functions read as never called. That is
/// how a suite ends up papered over with `qual:api`, which then hides whatever
/// is genuinely dead underneath.
///
/// Precise trigger, coarse payload: only at an invocation of one of *these*
/// macros does the caller harvest every ident as a possible callee. Doing it
/// for every macro invocation would let `assert_eq!(x, dead_helper)` vouch for
/// a dead function — the mistake that costs a real finding.
/// Integration: direct set, then the reach map decides the rest.
pub(crate) fn call_through_macros(
    bodies: &HashMap<String, TokenStream>,
) -> macro_params::CalledPositions {
    close_over_forwarders(bodies)
}

/// Grow the set until nothing new calls through.
///
/// One rule for every hop: a macro calls through when it applies a metavariable
/// itself, or hands one to a position an already-known macro calls. The
/// positions it calls are then read back onto *its* matcher, so the next hop
/// asks about the right argument — dropping them after the first hop let any
/// metavariable count again, and an invocation excused whatever was dead.
/// Operation: fixpoint over the definitions, own calls in the closures.
fn close_over_forwarders(bodies: &HashMap<String, TokenStream>) -> macro_params::CalledPositions {
    let mut through = macro_params::CalledPositions::new();
    for _ in 0..=bodies.len() {
        let fresh: Vec<(String, macro_params::CallShape)> = bodies
            .iter()
            .filter(|(name, _)| !through.contains_key(*name))
            .filter_map(|(name, body)| {
                macro_params::called_positions(body, &through).map(|p| (name.clone(), p))
            })
            .collect();
        if fresh.is_empty() {
            break;
        }
        through.extend(fresh);
    }
    through
}
