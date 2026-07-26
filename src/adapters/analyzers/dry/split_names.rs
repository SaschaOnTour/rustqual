//! Names split by the context they were seen in.
//!
//! Two collectors answer "which names does production use, and which only
//! tests": the call graph behind DRY-002 and the reference set behind DRY-006.
//! They differ in *what* counts as a use — a call/construction position versus
//! any occurrence — but not in how the split is kept: the same two sets, the
//! same production/test switch, the same per-file driver, the same
//! `#[cfg(test)] mod` scoping. That part lives here once.

use std::collections::HashSet;

use syn::visit::Visit;

use super::has_cfg_test;

/// The two sets plus the switch between them.
#[derive(Default)]
pub(crate) struct SplitNames {
    pub(crate) production: HashSet<String>,
    pub(crate) tests: HashSet<String>,
    pub(crate) in_test: bool,
}

impl SplitNames {
    /// The set for the current context.
    /// Operation: one branch, no own calls.
    pub(crate) fn target(&mut self) -> &mut HashSet<String> {
        if self.in_test {
            &mut self.tests
        } else {
            &mut self.production
        }
    }

    /// Enter an item that may be `#[cfg(test)]`, returning the context to
    /// restore afterwards. Test context is sticky: an item inside an already
    /// test-only scope stays test-only whatever its own attributes say.
    /// Operation: attribute check + flag update, no own calls.
    pub(crate) fn enter(&mut self, attrs: &[syn::Attribute]) -> bool {
        let previous = self.in_test;
        self.in_test = previous || has_cfg_test(attrs);
        previous
    }

    /// Operation: flag restore, no own calls.
    pub(crate) fn leave(&mut self, previous: bool) {
        self.in_test = previous;
    }
}

/// A visitor that keeps its names split by context.
pub(crate) trait SplitCollector {
    fn names(&mut self) -> &mut SplitNames;
}

/// Run `collector` over every file with the context set from `cfg_test_files`,
/// and hand back `(production, tests)`.
/// Operation: per-file context switch + visitor run, own calls in the closure.
pub(crate) fn collect_split<V>(
    parsed: &[(String, String, syn::File)],
    cfg_test_files: &HashSet<String>,
    collector: &mut V,
) -> (HashSet<String>, HashSet<String>)
where
    V: SplitCollector + for<'ast> Visit<'ast>,
{
    parsed.iter().for_each(|(path, _, file)| {
        collector.names().in_test = cfg_test_files.contains(path);
        syn::visit::visit_file(collector, file);
    });
    let names = collector.names();
    (
        std::mem::take(&mut names.production),
        std::mem::take(&mut names.tests),
    )
}
