//! Source-adapter sub-modules: how rustqual reads the world.
//!
//! `filesystem` walks the project tree and parses files; `watch` subscribes
//! to filesystem events for long-running re-analysis. These are the only
//! two ways the project currently acquires a working set of `syn::File`
//! values — everything else is derived from that input.

#![allow(dead_code, unused_imports)]

pub mod filesystem;
pub mod watch;

pub(crate) use filesystem::INVALID_QUAL_ALLOW_REASON_PREFIX;

#[cfg(test)]
mod tests;
