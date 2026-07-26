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

use super::dead_code_level;

/// The `dead_code` lint level in force. Rust resolves levels innermost-first,
/// so an inner `deny` really does revoke an outer `allow` — modelling the
/// context as a one-way flag would keep suppressing what the author re-armed.
/// Only `allow` excuses a declaration; `warn` and `deny` both mean "report it",
/// which is what rustqual does anyway. `forbid` is `Report` that a narrower
/// scope may not relax: for rustc an inner `allow` under it is an error, so
/// honouring one would silence something the compiler never would.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeadCodeLevel {
    Allow,
    Report,
    Forbid,
}

/// The level in force, tracked down the lexical scopes.
pub(crate) struct AllowScope {
    inherited: DeadCodeLevel,
}

impl Default for AllowScope {
    fn default() -> Self {
        Self {
            inherited: DeadCodeLevel::Report,
        }
    }
}

impl AllowScope {
    /// Start a file: its inner attributes (`#![allow(dead_code)]`) apply to
    /// everything below.
    /// Operation: level lookup, own call in the argument.
    pub(crate) fn enter_file(&mut self, attrs: &[syn::Attribute]) {
        self.inherited = dead_code_level(attrs).unwrap_or(DeadCodeLevel::Report);
    }

    /// Enter a scope-forming item, returning the level to restore afterwards.
    /// An explicit level on the item wins over the inherited one.
    /// Trivial: delegates to `effective`.
    pub(crate) fn enter(&mut self, attrs: &[syn::Attribute]) -> DeadCodeLevel {
        let previous = self.inherited;
        self.inherited = self.effective(attrs);
        previous
    }

    /// Operation: level restore, no own calls.
    pub(crate) fn leave(&mut self, previous: DeadCodeLevel) {
        self.inherited = previous;
    }

    /// Whether a declaration carrying `attrs` is excused — by its own level or
    /// by the one it inherits.
    /// Trivial: delegates to `effective`.
    pub(crate) fn covers(&self, attrs: &[syn::Attribute]) -> bool {
        self.effective(attrs) == DeadCodeLevel::Allow
    }

    /// The level in force for a declaration carrying `attrs`: its own, unless
    /// the surrounding scope forbids, which nothing narrower may relax.
    /// Operation: inherited-level dispatch, own call in the arm.
    fn effective(&self, attrs: &[syn::Attribute]) -> DeadCodeLevel {
        match self.inherited {
            DeadCodeLevel::Forbid => DeadCodeLevel::Forbid,
            inherited => dead_code_level(attrs).unwrap_or(inherited),
        }
    }
}
