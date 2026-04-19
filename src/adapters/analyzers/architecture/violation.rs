//! Shared value types for Architecture-Dimension matchers.
//!
//! A matcher returns zero or more `MatchLocation`s, each identifying one
//! occurrence of a rule violation with enough context for reporting.

/// The kind of match a matcher produced — mirrors the matcher identifier
/// so the reporting layer can render rule-appropriate details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViolationKind {
    /// Matched by `forbid_path_prefix`: a path beginning with a banned prefix.
    PathPrefix {
        /// The prefix that matched.
        prefix: String,
        /// The full rendered path that triggered the match.
        rendered_path: String,
    },
    /// Matched by `forbid_glob_import`: a `use foo::*` glob import.
    GlobImport {
        /// The path preceding the `*` in the import.
        base_path: String,
    },
    /// Matched by `forbid_method_call`: a call to a banned method name.
    ///
    /// Covers both direct dot-notation (`x.unwrap()`) and UFCS form
    /// (`Option::unwrap(x)`), identified by the final path segment.
    MethodCall {
        /// The matched method name (the banned entry from the rule list).
        name: String,
        /// "direct" for `x.name(...)`, "ufcs" for `Type::name(...)`.
        syntax: &'static str,
    },
    /// Matched by `forbid_macro_call`: a macro invocation `name!(...)`.
    MacroCall {
        /// The invoked macro's final path-segment name.
        name: String,
    },
}

/// One concrete occurrence of a matcher hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchLocation {
    /// Source file path (as passed to the matcher).
    pub file: String,
    /// 1-based line number of the offending token.
    pub line: usize,
    /// 0-based column of the offending token.
    pub column: usize,
    /// Specific match details.
    pub kind: ViolationKind,
}
