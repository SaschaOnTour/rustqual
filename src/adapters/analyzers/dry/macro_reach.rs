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

use crate::adapters::shared::file_visitor::{visit_all_files, FileVisitor};
use crate::adapters::shared::macro_tokens;

/// Macro name → every name its body mentions, following nested macro
/// invocations to their own bodies.
pub(crate) type MacroReach = HashMap<String, Vec<String>>;

/// Collect what each `macro_rules!` macro in the workspace reaches.
/// Integration: collection then transitive closure.
pub(crate) fn collect_macro_reach(parsed: &[(String, String, syn::File)]) -> MacroReach {
    let mut collector = MacroBodyCollector::default();
    visit_all_files(parsed, &mut collector);
    close_over_nested(collector.bodies)
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
            direct
                .iter()
                .filter_map(|n| bodies.get(n))
                .for_each(|nested| reached.extend(nested.iter().cloned()));
            changed |= reached.len() != before;
            out.insert(name.clone(), reached.into_iter().collect());
        }
        if !changed {
            break;
        }
    }
    out
}

/// Visitor recording each `macro_rules!` definition and the names in its body.
#[derive(Default)]
struct MacroBodyCollector {
    bodies: MacroReach,
}

impl FileVisitor for MacroBodyCollector {
    fn reset_for_file(&mut self, _file_path: &str) {}
}

impl<'ast> syn::visit::Visit<'ast> for MacroBodyCollector {
    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        let Some(name) = node.ident.as_ref().map(|i| i.to_string()) else {
            return;
        };
        let names: Vec<String> = macro_tokens::all_idents(&node.mac.tokens).collect();
        self.bodies.insert(name, names);
    }
}
