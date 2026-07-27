//! Suppression adapter — parses annotation comments from source text.
//!
//! Only one backend is currently supported: the `// qual:…` comment
//! family parsed line-by-line in `qual_allow`. Additional adapters
//! (e.g. attribute-based or config-level suppression) would become
//! siblings under this module.

pub mod qual_allow;

#[cfg(test)]
mod tests;
