//! `DeclaredFunction` — metadata about a declared function, shared across
//! analyzers (DRY dead-code analysis, TQ untested/SUT checks). A plain data
//! type with no analyzer dependency, so it lives in `shared/` rather than
//! inside one analyzer that others reach into.

/// A declared function with metadata for dead-code / test-quality analysis.
pub struct DeclaredFunction {
    pub name: String,
    pub qualified_name: String,
    pub file: String,
    pub line: usize,
    pub is_test: bool,
    pub is_main: bool,
    pub is_trait_impl: bool,
    /// Excused from the dead-code question by an attribute: an
    /// `#[allow(dead_code)]` in force at the declaration, or an export
    /// (`#[no_mangle]` and friends), which rustc treats as a live root.
    pub dead_code_exempt: bool,
    /// Whether this function is marked as public API via `// qual:api`.
    pub is_api: bool,
    /// Whether this function is marked as a test-only helper via
    /// `// qual:test_helper`. Narrowly excludes the DRY-002 `testonly`
    /// dead-code finding and TQ-003 (untested) without disabling
    /// other checks.
    pub is_test_helper: bool,
}
