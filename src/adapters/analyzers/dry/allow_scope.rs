//! The `#[allow(dead_code)]` in force at a declaration.
//!
//! Rust lint levels are inherited: `#![allow(dead_code)]` at the top of a file
//! covers everything in it, and the attribute on a module covers everything the
//! module declares — the generated-code idiom is one attribute on the `mod`,
//! not one per item. Reading only a declaration's own attributes therefore
//! reports items the author has already excused, and contradicts what both
//! dead-code checks document.
//!
//! DRY-002 (functions) and DRY-006 (types) each collect declarations with their
//! own visitor, so the inherited context lives here rather than in either.

use super::has_allow_dead_code;

/// Whether `#[allow(dead_code)]` is in force, tracked down the module tree.
#[derive(Default)]
pub(crate) struct AllowScope {
    inherited: bool,
}

impl AllowScope {
    /// Start a file: its inner attributes (`#![allow(dead_code)]`) apply to
    /// everything below.
    /// Operation: attribute check, no own calls.
    pub(crate) fn enter_file(&mut self, attrs: &[syn::Attribute]) {
        self.inherited = has_allow_dead_code(attrs);
    }

    /// Enter an item that may carry the attribute, returning the context to
    /// restore afterwards. Inheritance is one-way: an inner scope cannot
    /// revoke an outer `allow`.
    /// Operation: attribute check + flag update, no own calls.
    pub(crate) fn enter(&mut self, attrs: &[syn::Attribute]) -> bool {
        let previous = self.inherited;
        self.inherited = previous || has_allow_dead_code(attrs);
        previous
    }

    /// Operation: flag restore, no own calls.
    pub(crate) fn leave(&mut self, previous: bool) {
        self.inherited = previous;
    }

    /// Whether a declaration carrying `attrs` is excused — by its own attribute
    /// or by one it inherits.
    /// Operation: attribute check, no own calls.
    pub(crate) fn covers(&self, attrs: &[syn::Attribute]) -> bool {
        self.inherited || has_allow_dead_code(attrs)
    }
}
