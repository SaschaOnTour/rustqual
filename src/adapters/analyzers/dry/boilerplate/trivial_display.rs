//! BP-002: trivial `Display` — semantic matching. A body is trivial when it
//! is branch-free and every statement is a formatter write op: `write!` /
//! `writeln!`, `f.write_str(…)`, `f.write_char(…)`, or a `Display::fmt`
//! delegation. Multi-statement bodies count too, so the rule cannot be
//! dodged by splitting one `write!` into two `write_char` statements — the
//! historical evasion this check was blind to when it matched surface
//! syntax (one `write!` macro) instead of meaning.
//!
//! `[boilerplate].accepted_display_idioms` turns forms into declared policy:
//! a body whose every used idiom is accepted is no finding, while any other
//! trivial form keeps firing — the rule then enforces the house idiom
//! instead of going silent.

use std::collections::BTreeSet;
use syn::spanned::Spanned;

use super::{self_type_of, trait_name_of, BoilerplateFind};
use crate::config::sections::BoilerplateConfig;

/// One recognized statement in a trivial fmt body: a write op tagged with
/// its idiom (vocabulary name), or neutral filler (`Ok(())`).
enum WriteOp {
    Idiom(&'static str),
    Neutral,
}

const SUGGEST_WITH_CRATES: &str = "Derive it (derive_more::Display), declare the \
    project's house idiom in [boilerplate] accepted_display_idioms, or generate \
    the impls with one local macro_rules! — details: rustqual --explain BP-002";

const SUGGEST_NEUTRAL: &str = "Derive it with a derive macro, declare the \
    project's house idiom in [boilerplate] accepted_display_idioms, or generate \
    the impls with one local macro_rules! — details: rustqual --explain BP-002";

/// Detect trivial `impl Display` bodies (branch-free, write-only) whose
/// idioms are not all declared as accepted policy.
/// Operation: AST pattern matching logic; helper calls in closures.
pub(super) fn check_trivial_display(
    parsed: &[(String, String, syn::File)],
    config: &BoilerplateConfig,
) -> Vec<BoilerplateFind> {
    pattern_guard!("BP-002", config);
    let suggest = if config.suggest_crates {
        SUGGEST_WITH_CRATES
    } else {
        SUGGEST_NEUTRAL
    };
    let accepted = &config.accepted_display_idioms;
    parsed
        .iter()
        .flat_map(|(file, _, syntax)| {
            syntax.items.iter().filter_map({
                let file = file.clone();
                move |item| {
                    let syn::Item::Impl(imp) = item else {
                        return None;
                    };
                    if trait_name_of(imp)? != "Display" {
                        return None;
                    }
                    let idioms = trivial_display_idioms(single_fmt_method(imp)?)?;
                    if idioms.iter().all(|i| accepted.iter().any(|a| a == i)) {
                        return None;
                    }
                    Some(build_find(imp, &idioms, &file, suggest))
                }
            })
        })
        .collect()
}

/// Build the BP-002 find for one trivial impl, naming the idioms it uses.
/// Operation: struct construction.
fn build_find(
    imp: &syn::ItemImpl,
    idioms: &BTreeSet<&'static str>,
    file: &str,
    suggest: &str,
) -> BoilerplateFind {
    let used: Vec<&str> = idioms.iter().copied().collect();
    BoilerplateFind {
        pattern_id: "BP-002".to_string(),
        file: file.to_string(),
        line: imp.self_ty.span().start().line,
        struct_name: self_type_of(imp),
        description: format!(
            "Trivial Display implementation (branch-free, {} only)",
            used.join("/")
        ),
        suggestion: suggest.to_string(),
        suppressed: false,
    }
}

/// The impl's single method, when it is named `fmt`.
/// Operation: iterator logic, no own calls.
fn single_fmt_method(imp: &syn::ItemImpl) -> Option<&syn::ImplItemFn> {
    let mut fns = imp.items.iter().filter_map(|i| match i {
        syn::ImplItem::Fn(m) => Some(m),
        _ => None,
    });
    let first = fns.next()?;
    if fns.next().is_some() || first.sig.ident != "fmt" {
        return None;
    }
    Some(first)
}

/// The set of write idioms a trivial fmt body uses — `None` when any
/// statement is not a recognized write op (branching, locals, computation):
/// the conservative direction, the check stays silent.
/// Integration: formatter lookup + idiom collection.
fn trivial_display_idioms(method: &syn::ImplItemFn) -> Option<BTreeSet<&'static str>> {
    formatter_ident(method).and_then(|fmt| collect_idioms(&method.block, &fmt))
}

/// The formatter parameter's ident (the second fmt argument, usually `f`).
/// Operation: signature pattern match, no own calls.
fn formatter_ident(method: &syn::ImplItemFn) -> Option<String> {
    let syn::FnArg::Typed(pat_type) = method.sig.inputs.iter().nth(1)? else {
        return None;
    };
    let syn::Pat::Ident(pi) = &*pat_type.pat else {
        return None;
    };
    Some(pi.ident.to_string())
}

/// Collect the idioms of every statement, or `None` on the first statement
/// that is not a recognized write op.
/// Operation: iterator over statements; classification calls in closures.
fn collect_idioms(block: &syn::Block, fmt_ident: &str) -> Option<BTreeSet<&'static str>> {
    let ops: Option<Vec<WriteOp>> = block
        .stmts
        .iter()
        .map(|s| classify_stmt(s, fmt_ident))
        .collect();
    let idioms: BTreeSet<&'static str> = ops?
        .into_iter()
        .filter_map(|op| match op {
            WriteOp::Idiom(i) => Some(i),
            WriteOp::Neutral => None,
        })
        .collect();
    (!idioms.is_empty()).then_some(idioms)
}

/// Classify one statement as a write op.
/// Integration: dispatch on statement shape.
fn classify_stmt(stmt: &syn::Stmt, fmt_ident: &str) -> Option<WriteOp> {
    match stmt {
        syn::Stmt::Expr(e, _) => classify_expr(unwrap_try(e), fmt_ident),
        syn::Stmt::Macro(m) => classify_macro(&m.mac),
        _ => None,
    }
}

/// Strip any `?` layers: `f.write_char('[')?` classifies like the inner call.
/// Operation: loop unwrap, no own calls.
fn unwrap_try(expr: &syn::Expr) -> &syn::Expr {
    let mut e = expr;
    while let syn::Expr::Try(t) = e {
        e = &t.expr;
    }
    e
}

/// Classify one expression as a write op.
/// Integration: dispatch on expression shape.
fn classify_expr(expr: &syn::Expr, fmt_ident: &str) -> Option<WriteOp> {
    match expr {
        syn::Expr::Macro(m) => classify_macro(&m.mac),
        syn::Expr::MethodCall(mc) => classify_method_call(mc, fmt_ident),
        syn::Expr::Call(c) => classify_call(c, fmt_ident),
        _ => None,
    }
}

/// `write!` / `writeln!` → the `write_macro` idiom.
/// Operation: path check, no own calls.
fn classify_macro(mac: &syn::Macro) -> Option<WriteOp> {
    let last = mac.path.segments.last()?;
    if last.ident == "write" || last.ident == "writeln" {
        Some(WriteOp::Idiom("write_macro"))
    } else {
        None
    }
}

/// `f.write_str(…)` / `f.write_char(…)` on the formatter, or `self.x.fmt(f)`
/// delegation. A write on any other receiver is NOT the trivial idiom — it
/// writes somewhere else, which is real logic.
/// Operation: method-shape checks; receiver test in a closure.
fn classify_method_call(mc: &syn::ExprMethodCall, fmt_ident: &str) -> Option<WriteOp> {
    let is_fmt = |e: &syn::Expr| is_path_ident(e, fmt_ident);
    let method = mc.method.to_string();
    if method == "write_str" && is_fmt(&mc.receiver) {
        return Some(WriteOp::Idiom("write_str"));
    }
    if method == "write_char" && is_fmt(&mc.receiver) {
        return Some(WriteOp::Idiom("write_char"));
    }
    if method == "fmt" && mc.args.len() == 1 && is_fmt(&mc.args[0]) {
        return Some(WriteOp::Idiom("delegation"));
    }
    None
}

/// `Display::fmt(&self.x, f)` → delegation; `Ok(())` → neutral filler.
/// Operation: path/arg-shape checks; formatter test in a closure.
fn classify_call(call: &syn::ExprCall, fmt_ident: &str) -> Option<WriteOp> {
    let is_fmt = |e: &syn::Expr| is_path_ident(e, fmt_ident);
    let syn::Expr::Path(p) = &*call.func else {
        return None;
    };
    let segs: Vec<&syn::Ident> = p.path.segments.iter().map(|s| &s.ident).collect();
    let unit_arg = matches!(call.args.first(), Some(syn::Expr::Tuple(t)) if t.elems.is_empty());
    if segs.last().is_some_and(|s| *s == "Ok") && call.args.len() == 1 && unit_arg {
        return Some(WriteOp::Neutral);
    }
    let display_fmt =
        segs.len() >= 2 && *segs[segs.len() - 2] == "Display" && *segs[segs.len() - 1] == "fmt";
    if display_fmt && call.args.len() == 2 && is_fmt(&call.args[1]) {
        return Some(WriteOp::Idiom("delegation"));
    }
    None
}

/// True when `expr` is exactly the ident `name`, possibly `&`-referenced.
/// Operation: shape match, no own calls.
fn is_path_ident(expr: &syn::Expr, name: &str) -> bool {
    let inner = if let syn::Expr::Reference(r) = expr {
        &*r.expr
    } else {
        expr
    };
    matches!(inner, syn::Expr::Path(p) if p.path.is_ident(name))
}
