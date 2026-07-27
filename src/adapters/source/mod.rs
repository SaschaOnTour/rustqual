//! Source-adapter sub-modules: how rustqual reads the world.
//!
//! `filesystem` walks the project tree and parses files; `watch` subscribes
//! to filesystem events for long-running re-analysis. These are the only
//! two ways the project currently acquires a working set of `syn::File`
//! values — everything else is derived from that input.

pub mod filesystem;
pub mod watch;

#[cfg(test)]
mod tests;
