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

pub(crate) mod alias_substitution;
pub mod canonical;
pub mod combinators;
pub mod infer;
pub mod patterns;
pub mod resolve;
pub mod workspace_index;

// qual:api
pub use canonical::CanonicalType;

// qual:api
pub use combinators::combinator_return;

// qual:api
pub use infer::{infer_type, BindingLookup, FlatBindings, InferContext};

// qual:api
pub use patterns::{extract_bindings, extract_for_bindings};

// qual:api
pub use workspace_index::{build_workspace_type_index, WorkspaceIndexInputs, WorkspaceTypeIndex};

#[cfg(test)]
mod tests;
