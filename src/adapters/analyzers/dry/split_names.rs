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

use super::{has_cfg_test, has_test_attr};

/// The two sets plus the switch between them.
#[derive(Default)]
pub(crate) struct SplitNames {
    pub(crate) production: HashSet<String>,
    pub(crate) tests: HashSet<String>,
    pub(crate) in_test: bool,
}

impl SplitNames {
    /// The test set, whatever the current context. Doc examples are code
    /// `cargo test` compiles and runs, so what they name is a test reference
    /// even though the documented item is production code.
    /// Operation: field access, no own calls.
    pub(crate) fn test_target(&mut self) -> &mut HashSet<String> {
        &mut self.tests
    }

    /// The set for the current context.
    /// Operation: one branch, no own calls.
    pub(crate) fn target(&mut self) -> &mut HashSet<String> {
        if self.in_test {
            &mut self.tests
        } else {
            &mut self.production
        }
    }

    /// Enter an item that may be test-only, returning the context to restore
    /// afterwards. Both spellings count — `#[cfg(test)]` on a module, impl or
    /// free function, and `#[test]`-family attributes on a function — because
    /// the switch has to happen on every attributed item: a reference from a
    /// `#[cfg(test)] fn` that lands in the production set means a test-only
    /// declaration produces no finding at all.
    ///
    /// Test context is sticky: an item inside an already test-only scope stays
    /// test-only whatever its own attributes say.
    /// Operation: attribute checks + flag update, no own calls.
    pub(crate) fn enter(&mut self, attrs: &[syn::Attribute]) -> bool {
        let previous = self.in_test;
        self.in_test = previous || has_cfg_test(attrs) || has_test_attr(attrs);
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

/// The `Visit` methods that switch test context, one per node kind that can
/// carry `#[cfg(test)]`.
///
/// This exists as a list rather than as hand-written methods because the gap it
/// closes is always the same shape — *a node kind nobody thought of*. Three
/// review rounds found three of them (associated items, then fields, variants
/// and foreign items), each time in only one of the two collectors. Adding a
/// kind here fixes both at once, and the list itself is the documentation of
/// what is covered.
///
/// `visit_field` is **not** here: one collector reads serde attributes off a
/// field, so writing it out keeps that visible to the call graph rather than
/// hiding it behind a macro the analyzer cannot see through.
///
/// Not covered, deliberately: statement- and expression-level attributes
/// (`#[cfg(test)] let x = …`). They sit inside a function body, and a body that
/// is not itself test-only is production — the enclosing item already decided.
macro_rules! test_scoped_visits {
    () => {
        fn visit_item(&mut self, node: &'ast syn::Item) {
            let previous = self
                .names
                .enter(crate::adapters::shared::item_shape::item_attrs(node));
            syn::visit::visit_item(self, node);
            self.names.leave(previous);
        }

        fn visit_impl_item(&mut self, node: &'ast syn::ImplItem) {
            let previous = self
                .names
                .enter(crate::adapters::shared::item_shape::impl_item_attrs(node));
            syn::visit::visit_impl_item(self, node);
            self.names.leave(previous);
        }

        fn visit_trait_item(&mut self, node: &'ast syn::TraitItem) {
            let previous = self
                .names
                .enter(crate::adapters::shared::item_shape::trait_item_attrs(node));
            syn::visit::visit_trait_item(self, node);
            self.names.leave(previous);
        }

        fn visit_foreign_item(&mut self, node: &'ast syn::ForeignItem) {
            let previous =
                self.names
                    .enter(crate::adapters::shared::item_shape::foreign_item_attrs(
                        node,
                    ));
            syn::visit::visit_foreign_item(self, node);
            self.names.leave(previous);
        }

        fn visit_variant(&mut self, node: &'ast syn::Variant) {
            let previous = self.names.enter(&node.attrs);
            syn::visit::visit_variant(self, node);
            self.names.leave(previous);
        }
    };
}

pub(crate) use test_scoped_visits;

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
