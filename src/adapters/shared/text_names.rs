//! Identifiers hidden inside text.
//!
//! Two places name something without the lexer ever producing an identifier
//! token for it: a format string interpolates its arguments
//! (`format!("{PREFIX}{p}")` is one `Literal` to `proc_macro2`), and a doc
//! comment links to items (`/// see [`MAX`]` is `#[doc = "…"]`, one string).
//! Both are real references — deleting the named item breaks compilation in the
//! first case and the documentation in the second — and both are invisible to
//! any walk over tokens or the AST.
//!
//! The extraction is the same in both: find the delimited spans, then read the
//! identifier-shaped runs out of them. What differs is only the delimiter, and
//! how much of the span counts.

/// The names a format string interpolates. Every identifier-shaped run inside a
/// `{…}` placeholder is yielded, which covers both the argument and the spec
/// (`{value:>width$}` names two). `{{` is an escaped brace and starts no
/// placeholder; a positional `{0}` yields nothing, since a run must start like
/// an identifier. Operation: character scan, own call in the loop.
pub(crate) fn placeholder_names(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '{' {
            i += 1;
            continue;
        }
        if chars.get(i + 1) == Some(&'{') {
            i += 2;
            continue;
        }
        i += 1;
        let start = i;
        while i < chars.len() && chars[i] != '}' {
            i += 1;
        }
        out.extend(ident_runs(&chars[start..i]));
    }
    out
}

/// The item names a doc comment links to: the targets inside `[…]`, covering
/// ``[`Name`]``, `[Name]` and `[text](Name)`. Only bracketed spans count —
/// harvesting every word of prose would let any type whose name happens to
/// appear in a sentence keep itself alive.
/// Operation: character scan, own call in the loop.
pub(crate) fn doc_link_names(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '[' {
            i += 1;
            continue;
        }
        i += 1;
        let start = i;
        while i < chars.len() && chars[i] != ']' {
            i += 1;
        }
        out.extend(ident_runs(&chars[start..i]));
        out.extend(link_destination(&chars[i..]));
    }
    out
}

/// The `(Target)` immediately following a `]`, for the `[text](Target)` form.
/// Operation: shape check + run scan, own call at the end.
fn link_destination(rest: &[char]) -> Vec<String> {
    if rest.first() != Some(&']') || rest.get(1) != Some(&'(') {
        return Vec::new();
    }
    let end = rest.iter().position(|c| *c == ')').unwrap_or(rest.len());
    ident_runs(&rest[2..end])
}

/// Every identifier-shaped run in a line of code. Used for the inside of a doc
/// test's ``` fence, where the text really is code that `cargo test` compiles —
/// unlike prose, where a name is only a mention.
/// Operation: character collection + run scan, own call at the end.
pub(crate) fn code_names(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    ident_runs(&chars)
}

/// Whether a doc line opens or closes a code fence.
/// Operation: prefix test, no own calls.
pub(crate) fn is_doc_fence(text: &str) -> bool {
    text.trim_start().starts_with("```")
}

/// Identifier-shaped runs: start with a letter or `_`, continue with letters,
/// digits or `_`.
/// Operation: run scan, no own calls.
fn ident_runs(body: &[char]) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for &c in body {
        let extends = match current.is_empty() {
            true => c == '_' || c.is_alphabetic(),
            false => c == '_' || c.is_alphanumeric(),
        };
        match (extends, current.is_empty()) {
            (true, _) => current.push(c),
            (false, false) => out.push(std::mem::take(&mut current)),
            (false, true) => {}
        }
    }
    out.extend(Some(current).filter(|c| !c.is_empty()));
    out
}
