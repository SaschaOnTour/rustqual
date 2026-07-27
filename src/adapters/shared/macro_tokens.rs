//! Best-effort recovery of what a macro's opaque token stream references.
//!
//! `syn`'s visitor treats a macro body as an opaque [`proc_macro2::TokenStream`],
//! so calls, identifiers, and `self` uses inside `vec![…]`, `format!(…)`,
//! `rsx!{…}` etc. are invisible to every analyzer that only walks the AST.
//! This module is the single place that re-derives that information, so the
//! call-graph / cohesion / structural sites don't each reinvent a weaker copy
//! (the historical bug: ~7 independent `Punctuated<Expr, Comma>` parses, each
//! blind to the `;`-repeat and block-bodied forms).
//!
//! Two levels, matching the two safe directions:
//! - [`recover_exprs`] — *structured*: returns real [`syn::Expr`]s, so callers
//!   can run their normal expr visitors. Safe for everyone (no false positives,
//!   only best-effort recall). The architecture `forbid_*` matchers use ONLY
//!   this — a raw over-collecting fallback there would manufacture false
//!   forbidden-call violations.
//! - [`idents_in_call_position`] — *raw, positional*: for reachability consumers
//!   (dead-code, TQ) where a recovered call/component reference only ever
//!   *suppresses* a finding, never raises a false one. It harvests idents in
//!   call/construction position only, so DSL prop keys and locals don't pollute
//!   the call set.
//! - [`tokens_reference_ident`] — *raw, presence*: SLM's yes/no "does this body
//!   reference `self`" check.

use proc_macro2::{Delimiter, TokenStream, TokenTree};

use super::text_names::placeholder_names;

/// Best-effort extraction of the expressions embedded in a macro token stream.
/// Most macros accept comma-separated exprs (`assert!(a, b)`, `format!("{}", x)`),
/// but block-like bodies (`tokio::select! { … }`) and separator-`;` variants
/// (`vec![x; n]`) don't. Three strategies in order:
/// 1. Comma-separated `syn::Expr` list (covers ~90% of macro calls).
/// 2. Brace-wrapped parse as a `syn::Block` — extracts every statement
///    expression, covering block-bodied and `;`-separated forms.
/// 3. Single `syn::Expr` — for macros whose argument is one expression.
///
/// Silent-skips (returns empty) on total parse failure (extern-DSL macros with
/// custom grammar) — recover their calls via [`idents_in_call_position`] where
/// the consumer can safely over-collect.
/// Operation: parser dispatch over fallback strategies, no own calls.
pub fn recover_exprs(tokens: &TokenStream) -> Vec<syn::Expr> {
    use syn::parse::Parser;
    use syn::punctuated::Punctuated;
    use syn::Token;
    let parser = Punctuated::<syn::Expr, Token![,]>::parse_terminated;
    if let Ok(exprs) = parser.parse2(tokens.clone()) {
        return exprs.into_iter().collect();
    }
    let braced = quote::quote! { { #tokens } };
    if let Ok(block) = syn::parse2::<syn::Block>(braced) {
        return block.stmts.into_iter().flat_map(stmt_exprs).collect();
    }
    if let Ok(expr) = syn::parse2::<syn::Expr>(tokens.clone()) {
        return vec![expr];
    }
    Vec::new()
}

/// The expressions carried by a block statement (zero or more): a bare
/// expression; a `let`'s initialiser **plus** its `else { … }` diverging block
/// (let-else), so a call reachable only through the diverging branch is not
/// dropped; or a trailing-`;` macro statement (`format!(…);`) lifted back to an
/// `Expr::Macro` so nested macro calls inside a `;`-separated body survive.
/// `Stmt::Item` (e.g. a `const X = f();` nested in a macro body) is deliberately
/// dropped — an uncommon shape; its calls are recovered by the reachability
/// consumers' positional fallback. Operation: stmt-shape match, no own calls.
fn stmt_exprs(stmt: syn::Stmt) -> Vec<syn::Expr> {
    match stmt {
        syn::Stmt::Expr(e, _) => vec![e],
        syn::Stmt::Local(l) => l
            .init
            .map(|init| {
                let mut exprs = vec![*init.expr];
                if let Some((_, diverge)) = init.diverge {
                    exprs.push(*diverge);
                }
                exprs
            })
            .unwrap_or_default(),
        syn::Stmt::Macro(m) => vec![syn::Expr::Macro(syn::ExprMacro {
            attrs: m.attrs,
            mac: m.mac,
        })],
        _ => Vec::new(),
    }
}

/// Identifiers in `tokens` that sit in *call or construction position*,
/// recursing into nested groups (order unspecified):
/// - an `Ident` followed by a `(…)` group — a call (`helper(…)`), any case;
/// - an **UpperCamelCase** `Ident` followed by a `{…}` group — a struct literal
///   or DSL component render (`Component { … }`).
///
/// The UpperCamelCase gate on brace-groups is what separates a component/struct
/// render from a lowercase DSL element tag (`div { … }`, `button { … }`): a
/// lowercase tag is never a Rust function/struct, so harvesting it could only
/// mask an unused `fn div()` — excluding it keeps the over-collection tight. A
/// real lowercase function is still captured via its `(…)` call form.
///
/// This is the precise raw fallback for DSL macro bodies (`rsx!{ Component {
/// prop: x } }`) that [`recover_exprs`] cannot parse: it captures the
/// component/function reference while EXCLUDING prop keys, field names, locals,
/// type-path segments, element tags, and string contents — so a prop named like
/// a production function (`Widget { helper: "x" }`) does NOT spuriously mark
/// `helper` as called. The consumer intersects the result against known
/// function/component names. Operation: positional token-tree walk, no own calls.
pub fn idents_in_call_position(tokens: &TokenStream) -> impl Iterator<Item = String> {
    let mut out = Vec::new();
    let mut stack: Vec<TokenStream> = vec![tokens.clone()];
    while let Some(stream) = stack.pop() {
        let trees: Vec<TokenTree> = stream.into_iter().collect();
        for (i, tt) in trees.iter().enumerate() {
            match tt {
                TokenTree::Ident(id) if is_call_position(id, trees.get(i + 1)) => {
                    out.push(id.to_string());
                }
                TokenTree::Group(g) => stack.push(g.stream()),
                _ => {}
            }
        }
    }
    out.into_iter()
}

/// Whether `id`, given the token immediately after it, is in call/construction
/// position: a `(…)` group is a call (any case); a `{…}` group counts only when
/// `id` is UpperCamelCase (a component/struct, not a lowercase element tag).
/// Operation: delimiter + casing predicate, no own calls.
fn is_call_position(id: &proc_macro2::Ident, next: Option<&TokenTree>) -> bool {
    let Some(TokenTree::Group(g)) = next else {
        return false;
    };
    match g.delimiter() {
        Delimiter::Parenthesis => true,
        Delimiter::Brace => id
            .to_string()
            .chars()
            .next()
            .is_some_and(char::is_uppercase),
        _ => false,
    }
}

/// True if `name` appears as an identifier anywhere in `tokens` (recursing into
/// groups), or as a `{…}` placeholder in a string literal. Never fails — for
/// yes/no presence checks like SLM's "does this macro body reference `self`",
/// where parsing the body as exprs is both unnecessary and fragile (e.g.
/// `matches!(self, Variant(_))` won't parse as an expr list). A spurious hit
/// only errs toward "reference present", the safe direction for a
/// self-less-method check — which is also why the placeholders count:
/// `format!("{self:?}")` is one `Literal` to `proc_macro2`, and missing it
/// reported a method that plainly uses `self` as self-less.
/// Integration: token walk + placeholder extraction.
pub fn tokens_reference_ident(tokens: &TokenStream, name: &str) -> bool {
    let mut stack: Vec<TokenStream> = vec![tokens.clone()];
    while let Some(stream) = stack.pop() {
        for tt in stream {
            match tt {
                TokenTree::Ident(id) if id == name => return true,
                TokenTree::Group(g) => stack.push(g.stream()),
                TokenTree::Literal(lit) if names_placeholder(&lit, name) => return true,
                _ => {}
            }
        }
    }
    false
}

/// Whether a literal interpolates `name`.
/// Operation: placeholder lookup, own call hidden in the closure.
fn names_placeholder(lit: &proc_macro2::Literal, name: &str) -> bool {
    placeholder_names(&lit.to_string())
        .iter()
        .any(|p| p == name)
}

/// Every identifier in `tokens`, recursing into groups and into the
/// placeholders of string literals. The raw, maximally generous harvest — no
/// position or casing filter — for the question "does anything here refer to
/// this name at all". A spurious hit only errs toward "referenced", which for a
/// dead-declaration check is the safe direction: it suppresses a finding rather
/// than inventing one. Integration: token walk + placeholder extraction.
pub fn all_idents(tokens: &TokenStream) -> impl Iterator<Item = String> {
    let mut out = Vec::new();
    let mut stack: Vec<TokenStream> = vec![tokens.clone()];
    while let Some(stream) = stack.pop() {
        for tt in stream {
            match tt {
                TokenTree::Ident(id) => out.push(id.to_string()),
                TokenTree::Group(g) => stack.push(g.stream()),
                TokenTree::Literal(lit) => out.extend(placeholder_names(&lit.to_string())),
                _ => {}
            }
        }
    }
    out.into_iter()
}

/// Macros whose body is turned into tokens or text rather than run. `$f()`
/// inside one of these is not a call, and reading it as one lets the
/// invocation excuse a function nothing ever executes.
const NON_EXECUTING: [&str; 2] = ["stringify", "quote"];

/// Whether these tokens use a metavariable in callee position: `$name(…)`.
///
/// The transcriber of `macro_rules! step { ($t:path) => { $t(&make()); } }`
/// holds `$`, `t`, `(…)` as three consecutive trees. What the invocation binds
/// to `$t` is the function that really runs, and no walk over the invocation
/// can know that without first asking the definition — an ident followed by a
/// comma is not in call position.
///
/// A nested macro is descended into, because `println!("{}", $f())` really does
/// run `$f` — except for the ones that only quote their input: `stringify!($f())`
/// produces the string `"$f()"` and calls nothing. Skipping *all* nested macros
/// would be the safer-looking choice and the wrong one, since it would reopen
/// the case this predicate exists for.
/// Operation: positional token-tree scan, no own calls.
pub fn calls_through_metavariable(tokens: &TokenStream) -> bool {
    let mut stack: Vec<TokenStream> = vec![tokens.clone()];
    while let Some(stream) = stack.pop() {
        let trees: Vec<TokenTree> = stream.into_iter().collect();
        for (i, tt) in trees.iter().enumerate() {
            let quoted = i >= 2
                && matches!(&trees[i - 1], TokenTree::Punct(p) if p.as_char() == '!')
                && matches!(&trees[i - 2], TokenTree::Ident(id)
                    if NON_EXECUTING.contains(&id.to_string().as_str()));
            match tt {
                TokenTree::Group(g) if !quoted => stack.push(g.stream()),
                _ => {}
            }
            let is_dollar = matches!(tt, TokenTree::Punct(p) if p.as_char() == '$');
            let names_a_call = matches!(trees.get(i + 1), Some(TokenTree::Ident(_)))
                && matches!(trees.get(i + 2), Some(TokenTree::Group(g))
                    if g.delimiter() == proc_macro2::Delimiter::Parenthesis);
            if is_dollar && names_a_call {
                return true;
            }
        }
    }
    false
}
