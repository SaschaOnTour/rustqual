//! Shallow type-inference for call_parity receiver resolution.
//!
//! Public surface, in resolution order:
//! - `canonical::CanonicalType` — the inference vocabulary
//! - `resolve` — `syn::Type` → `CanonicalType` conversion
//! - `combinators` — stdlib `Result`/`Option`/`Future` return-type table
//! - `infer` — shallow inference engine over `syn::Expr`
//! - `patterns` — pattern-binding extraction for destructuring
//! - `workspace_index` — per-workspace type/method/field/trait/alias index
//!
//! Design reference: `docs/rustqual-design-receiver-type-inference.md`.

pub mod canonical;
pub mod combinators;
pub mod infer;
pub mod patterns;
pub mod resolve;
mod resolve_alias;
mod resolve_marker;

pub(crate) use resolve_alias::single_ident_of;
mod resolve_wrapper;
pub(crate) mod self_subst;
pub mod workspace_index;

pub use canonical::CanonicalType;

// rustc reports these as unused: the consumers reach them through a
// `use super::*` in the test modules, and the lint does not see through the
// glob. Removing them breaks the test build — `cargo fix` did exactly that
// once. Scoped to the import, not the module, so a genuinely unused one here
// still shows up.
#[allow(unused_imports)]
pub use combinators::combinator_return;

// rustc reports these as unused: the consumers reach them through a
// `use super::*` in the test modules, and the lint does not see through the
// glob. Removing them breaks the test build — `cargo fix` did exactly that
// once. Scoped to the import, not the module, so a genuinely unused one here
// still shows up.
#[allow(unused_imports)]
pub use infer::{infer_type, BindingLookup, FlatBindings, InferContext};

pub use patterns::{extract_bindings, extract_for_bindings};

pub use workspace_index::{
    build_workspace_type_index, MethodLocation, WorkspaceIndexInputs, WorkspaceTypeIndex,
};

#[cfg(test)]
mod tests;
