//! A source file as a Domain value.
//!
//! `SourceUnit` is the framework-free representation of a Rust source file
//! passed between Application use-cases and Adapter-layer analyzers. It
//! intentionally carries no AST — parsing happens in the adapter that
//! consumes the unit. This keeps the Domain free of `syn` and makes
//! `SourceUnit` trivially `Send + Sync`.
//!
//! Methods are exercised by tests but not yet consumed by the pipeline;
//! allow dead code at the module boundary until Phase 5.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// A Rust source file: its path and UTF-8 contents.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceUnit {
    path: PathBuf,
    content: String,
}

impl SourceUnit {
    /// Construct a new source unit.
    pub fn new(path: PathBuf, content: String) -> Self {
        Self { path, content }
    }

    /// The source file path (may be relative to the project root).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The source file contents as UTF-8 text.
    pub fn content(&self) -> &str {
        &self.content
    }
}
