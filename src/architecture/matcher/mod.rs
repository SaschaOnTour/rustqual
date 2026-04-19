//! Matchers for the Architecture-Dimension.
//!
//! Each matcher is an AST-level function that takes a parsed `syn::File`
//! (plus matcher-specific parameters) and returns all occurrences of a
//! rule violation as `MatchLocation` values.
//!
//! Matchers are pure (no I/O, no global state) so they can be unit-tested
//! in isolation with fixture source strings.

pub mod glob_import;
pub mod path_prefix;

pub use glob_import::find_glob_imports;
pub use path_prefix::find_path_prefix_matches;

#[cfg(test)]
mod tests;
