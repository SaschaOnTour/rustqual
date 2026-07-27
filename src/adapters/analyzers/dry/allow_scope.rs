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
    /// Start with the level this file inherits from the module that declares
    /// it — see `inherited_allow`. `Report` for a file no module chain reaches.
    /// Operation: struct construction, no own calls.
    pub(crate) fn with_baseline(inherited: DeadCodeLevel) -> Self {
        Self { inherited }
    }

    /// Start a file: its inner attributes (`#![allow(dead_code)]`) apply to
    /// everything below, over whatever it inherited — a file's own `#[deny]`
    /// revokes an `#[allow]` on the module that declares it, exactly as it
    /// would one item up in the same file.
    /// Trivial: delegates to `effective`.
    pub(crate) fn enter_file(&mut self, attrs: &[syn::Attribute]) {
        self.inherited = self.effective(attrs);
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

    /// Whether a declaration carrying `attrs` is excused — by its own lint
    /// level, by the one it inherits, or by being an export.
    /// Operation: two conditions, own calls in the operands.
    pub(crate) fn covers(&self, attrs: &[syn::Attribute]) -> bool {
        self.effective(attrs) == DeadCodeLevel::Allow || is_export_root(attrs)
    }

    /// The level in force for a declaration carrying `attrs`.
    /// Trivial: delegates to the free form.
    fn effective(&self, attrs: &[syn::Attribute]) -> DeadCodeLevel {
        level_under(self.inherited, attrs)
    }
}

/// Whether these attributes make the item reachable from outside the compiled
/// artefact, which rustc's own `dead_code` lint treats as a live root.
///
/// `#[no_mangle]`, `#[used]` and `#[export_name = "…"]` are how an FFI or
/// plugin boundary is spelled: the caller is a linker, not any line of Rust in
/// the workspace, so "nothing refers to it" is the normal state and reporting
/// it is a false finding. Since reachability, the mistake no longer costs one
/// line but everything the export names.
///
/// Rust 2024 requires the `#[unsafe(no_mangle)]` spelling, so both forms are
/// read — a check that knew only the bare one would expire at the next edition
/// bump.
/// Operation: attribute scan, own calls in the closure.
pub(crate) fn is_export_root(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        let path = a.path();
        EXPORT_ATTRS.iter().any(|n| path.is_ident(n))
            || (path.is_ident("unsafe") && wraps_export(a))
    })
}

/// The attribute names that make an item a linker-visible root.
const EXPORT_ATTRS: [&str; 3] = ["no_mangle", "used", "export_name"];

/// Whether an `#[unsafe(…)]` wrapper carries one of them.
/// Operation: token scan, own call in the closure.
fn wraps_export(attr: &syn::Attribute) -> bool {
    let syn::Meta::List(list) = &attr.meta else {
        return false;
    };
    crate::adapters::shared::macro_tokens::all_idents(&list.tokens)
        .any(|ident| EXPORT_ATTRS.contains(&ident.as_str()))
}

/// The level `attrs` set under an `inherited` one: their own, unless the
/// surrounding scope forbids, which nothing narrower may relax.
///
/// Free rather than a method because the same rule applies one level up, where
/// there is no scope object yet — `inherited_allow` combines a parent module's
/// level with a `mod` declaration's attributes before any file is walked.
/// Operation: inherited-level dispatch, own call in the arm.
pub(crate) fn level_under(inherited: DeadCodeLevel, attrs: &[syn::Attribute]) -> DeadCodeLevel {
    match inherited {
        DeadCodeLevel::Forbid => DeadCodeLevel::Forbid,
        inherited => dead_code_level(attrs).unwrap_or(inherited),
    }
}
