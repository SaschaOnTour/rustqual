//! Port: load source files from some root (filesystem, in-memory, etc.).

use crate::domain::SourceUnit;
use std::path::{Path, PathBuf};

/// Yields the set of source files a use-case should analyse.
///
/// Implementations must be thread-safe so analysers can parallelise over
/// the returned units via rayon.
pub trait SourceLoader: Send + Sync {
    /// Load all source files reachable from `root`.
    ///
    /// The meaning of `root` is up to the implementation: a directory path
    /// for the filesystem loader, a workspace descriptor for the
    /// workspace-aware loader, a fake root for tests.
    fn load(&self, root: &Path) -> Result<Vec<SourceUnit>, LoadError>;
}

/// Errors that a `SourceLoader` may report.
///
/// The variant set is kept small and semantic; library-specific errors
/// (e.g. `walkdir::Error`) stay inside the adapter implementing this port.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("root not found: {0}")]
    RootNotFound(PathBuf),
    #[error("i/o error reading {path}: {message}")]
    Io { path: PathBuf, message: String },
    #[error("utf-8 decode error in {path}: {message}")]
    DecodeError { path: PathBuf, message: String },
    #[error("loader refused root: {0}")]
    Refused(String),
}
