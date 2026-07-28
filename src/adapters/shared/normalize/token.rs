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
    /// Free function call — the callee's own name, for a single-segment path.
    /// A qualified callee contributes no token at all; see `norm_callee` for why
    /// naming it by its last segment would conflate more than it separates.
    ///
    /// The same rule as `MethodCall`, for the same reason: a name in call
    /// position carries the meaning. Without it a body whose whole content is
    /// *which* functions it names — a suite runner, a dispatch list, a
    /// registration table — normalised to a positional index per callee and so
    /// matched every other body of the same length exactly.
    Call(String),
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
