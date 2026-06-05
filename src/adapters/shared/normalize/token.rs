//! The normalized token vocabulary produced by the AST walk.

/// A normalized AST token with variable names replaced by positional indices,
/// literal values erased to type placeholders, and structural tokens preserved.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NormalizedToken {
    /// Control flow keyword (if, for, while, match, loop, return, break, continue, let, else, etc.)
    Keyword(&'static str),
    /// Binary/unary/assignment operator as its token string.
    Operator(&'static str),
    /// Variable/parameter name replaced with first-seen positional index.
    Ident(usize),
    /// Method call — name preserved (structurally significant).
    MethodCall(String),
    /// Field access (e.g. self.field_name) — name preserved.
    FieldAccess(String),
    /// Integer literal (value erased).
    IntLit,
    /// Float literal (value erased).
    FloatLit,
    /// String/byte-string literal (value erased).
    StrLit,
    /// Boolean literal — value preserved (semantically significant).
    BoolLit(bool),
    /// Char/byte literal (value erased).
    CharLit,
    /// Macro invocation — name preserved.
    MacroCall(String),
    /// Statement terminator.
    Semi,
}
