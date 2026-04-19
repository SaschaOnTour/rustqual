//! Per-check helpers for the Trait-Signature rule.
//!
//! Each function receives the file path, the trait AST node, the compiled
//! rule, and an output vector; it appends its `MatchLocation`s into that
//! vector. All seven checks live here so the orchestrator in `mod.rs`
//! stays a simple dispatch integration.

use super::rendering::{receiver_kind, render_type, render_type_param_bound};
use super::CompiledTraitContract;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use syn::spanned::Spanned;

/// Flag methods whose receiver kind is not in `rule.receiver_may_be`.
/// Operation: per-method receiver classification.
pub(super) fn check_receiver(
    path: &str,
    t: &syn::ItemTrait,
    rule: &CompiledTraitContract,
    out: &mut Vec<MatchLocation>,
) {
    let Some(allowed) = &rule.receiver_may_be else {
        return;
    };
    if allowed.iter().any(|a| a == "any") {
        return;
    }
    trait_methods(t).iter().for_each(|m| {
        let Some(kind) = receiver_kind(&m.sig) else {
            return;
        };
        if !allowed.iter().any(|a| a == kind) {
            out.push(hit(
                path,
                t,
                "receiver",
                format!("{} has {kind} receiver", m.sig.ident),
            ));
        }
    });
}

/// Flag methods that are not declared `async`.
/// Operation: per-method asyncness inspection.
pub(super) fn check_async(
    path: &str,
    t: &syn::ItemTrait,
    rule: &CompiledTraitContract,
    out: &mut Vec<MatchLocation>,
) {
    if rule.methods_must_be_async != Some(true) {
        return;
    }
    trait_methods(t).iter().for_each(|m| {
        if m.sig.asyncness.is_none() {
            out.push(hit(
                path,
                t,
                "async",
                format!("{} is not async", m.sig.ident),
            ));
        }
    });
}

/// Flag return types whose rendered form contains any forbidden substring.
/// Operation: per-method return-type stringification + substring match.
pub(super) fn check_return_type(
    path: &str,
    t: &syn::ItemTrait,
    rule: &CompiledTraitContract,
    out: &mut Vec<MatchLocation>,
) {
    if rule.forbidden_return_type_contains.is_empty() {
        return;
    }
    trait_methods(t).iter().for_each(|m| {
        let rendered = match &m.sig.output {
            syn::ReturnType::Type(_, ty) => render_type(ty),
            _ => return,
        };
        rule.forbidden_return_type_contains
            .iter()
            .filter(|s| rendered.contains(s.as_str()))
            .for_each(|s| {
                out.push(hit(
                    path,
                    t,
                    "return_type",
                    format!("{} returns type containing {s:?}", m.sig.ident),
                ));
            });
    });
}

/// Flag methods whose parameter list lacks the required type substring.
/// Operation: per-method param iteration with substring match.
pub(super) fn check_required_param(
    path: &str,
    t: &syn::ItemTrait,
    rule: &CompiledTraitContract,
    out: &mut Vec<MatchLocation>,
) {
    let Some(required) = &rule.required_param_type_contains else {
        return;
    };
    trait_methods(t).iter().for_each(|m| {
        let has_required = m.sig.inputs.iter().any(|arg| match arg {
            syn::FnArg::Typed(pt) => render_type(&pt.ty).contains(required.as_str()),
            _ => false,
        });
        if !has_required {
            out.push(hit(
                path,
                t,
                "required_param",
                format!("{} lacks a {required:?} parameter", m.sig.ident),
            ));
        }
    });
}

/// Flag missing required supertrait bounds on the trait.
/// Operation: rendered supertrait list + per-required substring match.
pub(super) fn check_supertraits(
    path: &str,
    t: &syn::ItemTrait,
    rule: &CompiledTraitContract,
    out: &mut Vec<MatchLocation>,
) {
    if rule.required_supertraits_contain.is_empty() {
        return;
    }
    let rendered: String = t
        .supertraits
        .iter()
        .map(render_type_param_bound)
        .collect::<Vec<_>>()
        .join(" + ");
    rule.required_supertraits_contain
        .iter()
        .filter(|req| !rendered.contains(req.as_str()))
        .for_each(|req| {
            out.push(hit(
                path,
                t,
                "supertrait",
                format!("supertrait list missing {req:?}"),
            ));
        });
}

/// Conservative object-safety check: flag `Self` return and method-level generics.
/// Operation: per-method shape inspection.
pub(super) fn check_object_safety(
    path: &str,
    t: &syn::ItemTrait,
    rule: &CompiledTraitContract,
    out: &mut Vec<MatchLocation>,
) {
    if rule.must_be_object_safe != Some(true) {
        return;
    }
    trait_methods(t).iter().for_each(|m| {
        if returns_self(&m.sig.output) {
            out.push(hit(
                path,
                t,
                "object_safety",
                format!("{} returns Self", m.sig.ident),
            ));
        } else if !m.sig.generics.params.is_empty() {
            out.push(hit(
                path,
                t,
                "object_safety",
                format!("{} has method-level generics", m.sig.ident),
            ));
        }
    });
}

/// Flag enum variants of the trait's error return type that match forbidden substrings.
/// Operation: find local error type, scan variants.
pub(super) fn check_error_variants(
    path: &str,
    t: &syn::ItemTrait,
    ast: &syn::File,
    rule: &CompiledTraitContract,
    out: &mut Vec<MatchLocation>,
) {
    if rule.forbidden_error_variant_contains.is_empty() {
        return;
    }
    trait_methods(t).iter().for_each(|m| {
        let Some(error_name) = extract_error_name(&m.sig.output, &rule.error_types) else {
            return;
        };
        let Some(enum_item) = find_enum_in_file(ast, &error_name) else {
            return;
        };
        enum_item
            .variants
            .iter()
            .flat_map(|v| {
                v.fields
                    .iter()
                    .map(|f| render_type(&f.ty))
                    .collect::<Vec<_>>()
            })
            .for_each(|rendered| {
                rule.forbidden_error_variant_contains
                    .iter()
                    .filter(|s| rendered.contains(s.as_str()))
                    .for_each(|s| {
                        out.push(hit(
                            path,
                            t,
                            "error_variant",
                            format!("{error_name} variant contains {s:?}"),
                        ));
                    });
            });
    });
}

// ── internal helpers ─────────────────────────────────────────────────

/// Does the return type name `Self`?
/// Operation: pattern-match on Type::Path last segment.
fn returns_self(output: &syn::ReturnType) -> bool {
    match output {
        syn::ReturnType::Type(_, ty) => render_type(ty) == "Self",
        _ => false,
    }
}

/// Extract the error-type name from a `Result<T, E>` return, via explicit
/// `error_types` override or the `…Error` naming convention.
/// Operation: pattern-match on Type::Path generics.
fn extract_error_name(output: &syn::ReturnType, explicit: &[String]) -> Option<String> {
    let syn::ReturnType::Type(_, ty) = output else {
        return None;
    };
    let syn::Type::Path(tp) = ty.as_ref() else {
        return None;
    };
    let segment = tp.path.segments.last()?;
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    let err_arg = args.args.iter().nth(1)?;
    let syn::GenericArgument::Type(err_ty) = err_arg else {
        return None;
    };
    let name = render_type(err_ty);
    if !explicit.is_empty() {
        if explicit.contains(&name) {
            return Some(name);
        }
        return None;
    }
    if name.ends_with("Error") {
        Some(name)
    } else {
        None
    }
}

/// Find a top-level `enum Name { … }` in a file.
/// Operation: iterator chain filter.
fn find_enum_in_file<'a>(ast: &'a syn::File, name: &str) -> Option<&'a syn::ItemEnum> {
    ast.items.iter().find_map(|item| match item {
        syn::Item::Enum(e) if e.ident == name => Some(e),
        _ => None,
    })
}

/// Collect all method items of a trait (skips associated types and consts).
/// Operation: iterator-chain filter on TraitItem::Fn.
pub(super) fn trait_methods(t: &syn::ItemTrait) -> Vec<&syn::TraitItemFn> {
    t.items
        .iter()
        .filter_map(|item| match item {
            syn::TraitItem::Fn(f) => Some(f),
            _ => None,
        })
        .collect()
}

/// Construct a TraitContract hit anchored at the trait's name span.
/// Operation: MatchLocation construction.
fn hit(path: &str, t: &syn::ItemTrait, check: &'static str, detail: String) -> MatchLocation {
    let span = t.ident.span().start();
    MatchLocation {
        file: path.to_string(),
        line: span.line,
        column: span.column,
        kind: ViolationKind::TraitContract {
            trait_name: t.ident.to_string(),
            check,
            detail,
        },
    }
}
