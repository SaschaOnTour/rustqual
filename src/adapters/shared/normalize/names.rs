//! Identifier bookkeeping for one normalised body: the positional index of
//! every name, and which names are locals.
//!
//! Two questions about the same thing, deliberately kept in one type and
//! answered from different state. *Which index does this name get* is the
//! alpha-renaming, assigned at first occurrence. *Is this name a local* decides
//! whether a callee keeps its name, and it must not consume an index — seeding
//! parameters into the index map made the numbering depend on how many
//! parameters a function declares, so one unused parameter shifted everything.

use std::collections::{HashMap, HashSet};

use syn::visit::Visit;

/// The positional index each name was given.
#[derive(Default)]
pub(super) struct Aliases {
    index_of: HashMap<String, usize>,
    next_index: usize,
}

impl Aliases {
    /// The positional index of `name`, assigned on first encounter.
    /// Operation: map lookup or insert, no own calls.
    pub(super) fn index(&mut self, name: &str) -> usize {
        if let Some(&id) = self.index_of.get(name) {
            return id;
        }
        let id = self.next_index;
        self.next_index += 1;
        self.index_of.insert(name.to_string(), id);
        id
    }
}

/// The names in scope while a body is walked.
#[derive(Default)]
pub(super) struct Scope {
    /// What the signature binds — in scope for the whole body.
    params: HashSet<String>,
    /// What the body has bound so far and not yet left behind.
    bound: HashSet<String>,
}

impl Scope {
    /// Whether `name` is a local here. In callee position that is what
    /// separates a callback from a free function; nothing in the token stream
    /// itself does.
    /// Operation: two set lookups, no own calls.
    pub(super) fn is_bound(&self, name: &str) -> bool {
        self.params.contains(name) || self.bound.contains(name)
    }

    /// Record a name as in scope from here on.
    /// Operation: set insert, no own calls.
    pub(super) fn bind(&mut self, name: &str) {
        self.bound.insert(name.to_string());
    }

    /// Read the parameter names, including every binding a pattern introduces
    /// at any depth: `fn f((cb, x): (F, u8))` binds `cb` just as a plain
    /// parameter would.
    /// Operation: pattern walk, own calls hidden in the visitor.
    pub(super) fn seed(&mut self, sig: &syn::Signature) {
        let mut binds = ParamBindings::default();
        sig.inputs.iter().for_each(|arg| binds.visit_fn_arg(arg));
        self.params = binds.names;
    }

    /// The bound set as it stands, to restore when a scope ends.
    /// Operation: clone, no own calls.
    pub(super) fn snapshot(&self) -> HashSet<String> {
        self.bound.clone()
    }

    /// Put the bound set back to `saved`.
    /// Operation: assignment, no own calls.
    pub(super) fn restore(&mut self, saved: HashSet<String>) {
        self.bound = saved;
    }

    /// What has been bound since `saved` was taken.
    /// Operation: set difference, no own calls.
    pub(super) fn bound_since(&self, saved: &HashSet<String>) -> Vec<String> {
        self.bound.difference(saved).cloned().collect()
    }

    /// Bring names back into scope, after holding them out of one.
    /// Operation: extend, no own calls.
    pub(super) fn rebind(&mut self, names: Vec<String>) {
        self.bound.extend(names);
    }
}

/// Collects every name a parameter pattern binds, at any depth.
#[derive(Default)]
struct ParamBindings {
    names: HashSet<String>,
}

impl<'ast> Visit<'ast> for ParamBindings {
    fn visit_pat_ident(&mut self, node: &'ast syn::PatIdent) {
        self.names.insert(node.ident.to_string());
        syn::visit::visit_pat_ident(self, node);
    }
}
