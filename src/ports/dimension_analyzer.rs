//! Port: a dimension analyzer produces `Finding`s for a parsed workspace.
//!
//! Each of the seven quality dimensions (IOSP, Complexity, DRY, SRP,
//! Coupling, Test Quality, Architecture) is represented in the Adapter
//! layer by a struct implementing this port. The Application layer holds
//! a `Vec<Box<dyn DimensionAnalyzer>>` and iterates over them — dimensions
//! become additive without touching use-case logic.
//!
//! Why not pass `&[SourceUnit]` directly?
//! The analyzers consume a parsed AST (`syn::File`) plus the original
//! content and path. A shared context type (`AnalysisContext`) captures
//! that triple and lets the Application layer cache parse results across
//! dimensions instead of re-parsing for each adapter.

use crate::config::Config;
use crate::domain::Finding;

/// A parsed source file with metadata the analyzers need.
///
/// Produced once by the Application layer after `SourceLoader` yields
/// raw `SourceUnit`s; shared by every analyzer invocation for the run.
pub struct ParsedFile {
    /// File path, normalised to forward slashes, relative to the analysis root.
    pub path: String,
    /// Raw source text. Analyzers that need line-level info (e.g. the
    /// suppression parser, length metrics) read from here directly.
    pub content: String,
    /// Parsed `syn::File` AST.
    pub ast: syn::File,
}

/// The bundle passed to every analyzer for one run.
///
/// Borrowing only; the Application layer owns the backing data. Adapters
/// are free to ignore whichever field does not concern them (e.g. a
/// coupling analyzer never reads `config.complexity`).
pub struct AnalysisContext<'a> {
    pub files: &'a [ParsedFile],
    pub config: &'a Config,
}

/// Port: produce findings for one dimension over one workspace.
///
/// Implementations must be thread-safe so the Application layer can run
/// them in parallel. They must not do I/O; reading files belongs to the
/// `SourceLoader` port.
pub trait DimensionAnalyzer: Send + Sync {
    /// Human-readable dimension name (e.g. `"iosp"`, `"architecture"`).
    /// Stable across rustqual versions; used in reports and suppression
    /// strings.
    fn dimension_name(&self) -> &'static str;

    /// Analyze the workspace and return every finding this dimension
    /// produces — suppressed or not. Suppression is applied at a later
    /// stage by the Application layer using the common `Suppression`
    /// data type.
    fn analyze(&self, ctx: &AnalysisContext<'_>) -> Vec<Finding>;
}
