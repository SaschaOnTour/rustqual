//! Suppression adapter — parses annotation comments from source text.
//!
//! Only one backend is currently supported: the `// qual:…` comment
//! family parsed line-by-line in `qual_allow`. Additional adapters
//! (e.g. attribute-based or config-level suppression) would become
//! siblings under this module.

#![allow(dead_code, unused_imports)]

pub mod qual_allow;
