//! Shallow type-inference for call_parity receiver resolution.
//!
//! Stage 1 (v1.2.0) provides the workspace type index used by the
//! inference engine. The engine itself lands in Task 1.3; for now this
//! module exposes `WorkspaceTypeIndex` + its builder and the
//! `CanonicalType` vocabulary.
//!
//! Design reference: `docs/rustqual-design-receiver-type-inference.md`.
//! Plan: `~/.claude/plans/cached-noodling-frog.md`.

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
pub use workspace_index::{build_workspace_type_index, WorkspaceTypeIndex};

#[cfg(test)]
mod tests;
