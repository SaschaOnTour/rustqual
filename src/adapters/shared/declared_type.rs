//! `DeclaredType` — metadata about a declared type or constant, shared by the
//! DRY-006 dead-type check and the stale-marker verification in `app/`. A plain
//! data type with no analyzer dependency, mirroring [`super::declared_function`].

/// What kind of declaration this is. Carried so a finding can name what it
/// found — "struct `Foo` is never used" reads very differently from
/// "const `MAX` is never used".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeItemKind {
    Struct,
    Enum,
    Union,
    TypeAlias,
    Const,
    Static,
}

impl TypeItemKind {
    /// The word used in messages and structured output.
    /// Operation: variant → label, no own calls.
    pub fn label(self) -> &'static str {
        match self {
            TypeItemKind::Struct => "struct",
            TypeItemKind::Enum => "enum",
            TypeItemKind::Union => "union",
            TypeItemKind::TypeAlias => "type alias",
            TypeItemKind::Const => "const",
            TypeItemKind::Static => "static",
        }
    }
}

/// A declared type or constant with the metadata DRY-006 needs.
pub struct DeclaredType {
    pub name: String,
    pub kind: TypeItemKind,
    pub file: String,
    pub line: usize,
    /// Declared in test code — a `#[cfg(test)]` module or a test-only file.
    pub is_test: bool,
    /// Excused from the dead-code question by an attribute: an
    /// `#[allow(dead_code)]` in force at the declaration, or an export
    /// (`#[no_mangle]` and friends), which rustc treats as a live root.
    pub dead_code_exempt: bool,
    /// Marked `// qual:api`: an entry point whose consumers live outside the
    /// analysed code, so having no in-workspace user is expected.
    pub is_api: bool,
    /// Marked `// qual:test_helper`: excludes the `test_only` finding, the
    /// same narrow purpose the marker has for functions.
    pub is_test_helper: bool,
}
