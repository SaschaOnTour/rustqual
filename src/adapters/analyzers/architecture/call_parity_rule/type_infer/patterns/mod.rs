//! Pattern-binding extraction — Task 1.4.
//!
//! `extract_bindings(pat, matched_type, ctx)` walks a `syn::Pat` and
//! returns the `(name, canonical_type)` pairs a `let`/`if let`/`while
//! let`/`let … else`/`match`-arm introduces into scope. For `for` loops,
//! `extract_for_bindings(pat, iter_type, ctx)` extracts the element type
//! from the iterator first, then delegates to the general pattern walker.
//!
//! Supported patterns (Stage 1):
//! - `Pat::Ident(x)` / `Pat::Wild` — base cases
//! - `Pat::Type(_: T)` — annotation overrides matched type
//! - `Pat::Reference` / `Pat::Paren` — transparent wrappers
//! - `Pat::Tuple` — tuples are `Opaque` (no tuple type tracking)
//! - `Pat::TupleStruct(Some|Ok|Err)` — Option/Result variant unwrap
//! - `Pat::Struct(S { field, … })` — field-type lookup via index
//! - `Pat::Slice([a, b, ..])` — element-type from `Slice(T)`
//! - `Pat::Or` — takes first branch's bindings (all branches should bind same names)
//!
//! Scope limits:
//! - User-defined enum variants beyond `Option`/`Result` yield `Opaque`.
//! - Tuple-pair destructuring from `HashMap` iteration doesn't recover
//!   K/V separately (we only track V).

pub mod destructure;
pub mod iterator;

// qual:api
pub use destructure::extract_bindings;

// qual:api
pub use iterator::extract_for_bindings;
