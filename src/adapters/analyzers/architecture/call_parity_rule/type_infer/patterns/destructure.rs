//! Pattern-binding walker. Produces `(name, type)` pairs from a
//! `syn::Pat` given the type of the expression being matched.
//!
//! The walker is recursive (patterns nest — `Some(Ctx { id })` binds
//! `id` to the field type inside an `Option<Ctx>`) but Stage 1 keeps
//! the cases flat: each pattern variant has a dedicated handler and
//! the dispatch lives in `collect`.

use super::super::canonical::CanonicalType;
use super::super::infer::InferContext;
use super::super::resolve::{resolve_type, ResolveContext};

// qual:api
/// Extract `(binding_name, canonical_type)` pairs from a pattern matched
/// against a value of `matched_type`. Integration: delegates to `collect`
/// which dispatches over pattern variants.
pub fn extract_bindings(
    pat: &syn::Pat,
    matched_type: &CanonicalType,
    ctx: &InferContext<'_>,
) -> Vec<(String, CanonicalType)> {
    let mut out = Vec::new();
    collect(pat, matched_type, ctx, &mut out);
    out
}

// qual:recursive
/// Dispatch on the pattern variant and delegate to the matching handler.
/// Integration: pure dispatch — each arm is a single delegation call.
fn collect(
    pat: &syn::Pat,
    matched_type: &CanonicalType,
    ctx: &InferContext<'_>,
    out: &mut Vec<(String, CanonicalType)>,
) {
    match pat {
        syn::Pat::Ident(pi) => bind_ident(pi, matched_type, out),
        syn::Pat::Type(pt) => bind_annotated(pt, ctx, out),
        syn::Pat::Reference(r) => collect(&r.pat, matched_type, ctx, out),
        syn::Pat::Paren(p) => collect(&p.pat, matched_type, ctx, out),
        syn::Pat::Tuple(t) => bind_tuple(t, ctx, out),
        syn::Pat::TupleStruct(ts) => bind_tuple_struct(ts, matched_type, ctx, out),
        syn::Pat::Struct(s) => bind_struct(s, matched_type, ctx, out),
        syn::Pat::Slice(s) => bind_slice(s, matched_type, ctx, out),
        syn::Pat::Or(o) => bind_or_first(o, matched_type, ctx, out),
        _ => {}
    }
}

/// `Pat::Ident(x)` — the simplest case: bind the name to the matched
/// type. `Opaque` bindings are still recorded so shadowing works
/// correctly (a later `ctx.bindings.lookup(x)` sees `Some(Opaque)`,
/// which callers can treat as "known unresolvable").
///
/// Syn-level ambiguity: `None` as a pattern is represented as
/// `Pat::Ident("None")`, not a distinct variant pattern. We specifically
/// suppress binding when the ident is `None` and the matched type is
/// `Option<_>` — the only case where this disambiguation can be made
/// statically. Other uppercase idents (`Some` can't appear without a
/// payload, `Ok`/`Err` always carry args and thus parse as
/// `Pat::TupleStruct`) don't need special handling. Operation.
fn bind_ident(
    pi: &syn::PatIdent,
    matched_type: &CanonicalType,
    out: &mut Vec<(String, CanonicalType)>,
) {
    let name = pi.ident.to_string();
    if is_variant_like(&name, matched_type) {
        return;
    }
    out.push((name, matched_type.clone()));
}

/// True for identifiers that are unambiguously a stdlib enum-variant
/// pattern (not a binding) given the matched type. Operation.
fn is_variant_like(name: &str, matched_type: &CanonicalType) -> bool {
    matches!((name, matched_type), ("None", CanonicalType::Option(_)))
}

/// `Pat::Type(inner: T)` — the annotation overrides the inferred type.
/// `let x: Session = returns_opaque()` produces `x: Session`.
/// Operation: resolve the annotation, re-enter via closure.
fn bind_annotated(
    pt: &syn::PatType,
    ctx: &InferContext<'_>,
    out: &mut Vec<(String, CanonicalType)>,
) {
    let resolve = |ty: &syn::Type| {
        let rctx = ResolveContext {
            file: ctx.file,
            mod_stack: ctx.mod_stack,
            type_aliases: Some(&ctx.workspace.type_aliases),
            transparent_wrappers: Some(&ctx.workspace.transparent_wrappers),
            workspace_files: ctx.workspace_files,
            alias_param_subs: None,
        };
        resolve_type(ty, &rctx)
    };
    let annotated = resolve(&pt.ty);
    collect(&pt.pat, &annotated, ctx, out);
}

/// `Pat::Tuple((a, b, c))` — Stage 1 doesn't track tuple types, so every
/// element receives `Opaque`. The per-element recursion may still bind
/// names (they'll be `Opaque`-typed). Operation.
fn bind_tuple(t: &syn::PatTuple, ctx: &InferContext<'_>, out: &mut Vec<(String, CanonicalType)>) {
    for elem in &t.elems {
        collect(elem, &CanonicalType::Opaque, ctx, out);
    }
}

/// `Pat::TupleStruct(Variant(sub, sub, …))` — for `Some`/`Ok`/`Err` we
/// know the inner type from the matched wrapper; for user-defined
/// variants we fall back to `Opaque`. Operation.
fn bind_tuple_struct(
    ts: &syn::PatTupleStruct,
    matched_type: &CanonicalType,
    ctx: &InferContext<'_>,
    out: &mut Vec<(String, CanonicalType)>,
) {
    let variant = ts
        .path
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default();
    let inner = variant_inner_type(&variant, matched_type);
    for elem in &ts.elems {
        collect(elem, &inner, ctx, out);
    }
}

/// Map a variant name + wrapper type to the inner payload type. Only
/// stdlib variants are recognised; user enums yield `Opaque`.
/// Operation: pure lookup.
fn variant_inner_type(variant: &str, matched_type: &CanonicalType) -> CanonicalType {
    match (variant, matched_type) {
        ("Some", CanonicalType::Option(inner)) => (**inner).clone(),
        ("Ok", CanonicalType::Result(inner)) => (**inner).clone(),
        // Err carries the E-side, which we erase in `CanonicalType::Result`.
        ("Err", CanonicalType::Result(_)) => CanonicalType::Opaque,
        _ => CanonicalType::Opaque,
    }
}

/// `Pat::Struct(T { field, field_alias: x, … })` — look up each named
/// field in the workspace struct-field index. Operation.
fn bind_struct(
    s: &syn::PatStruct,
    matched_type: &CanonicalType,
    ctx: &InferContext<'_>,
    out: &mut Vec<(String, CanonicalType)>,
) {
    let CanonicalType::Path(segs) = matched_type else {
        return;
    };
    let struct_key = segs.join("::");
    for field in &s.fields {
        let syn::Member::Named(ident) = &field.member else {
            continue;
        };
        let field_name = ident.to_string();
        let field_type = ctx
            .workspace
            .struct_field(&struct_key, &field_name)
            .cloned()
            .unwrap_or(CanonicalType::Opaque);
        collect(&field.pat, &field_type, ctx, out);
    }
}

/// `Pat::Slice([a, b, rest @ ..])` — element type comes from the matched
/// `Slice(T)`; `Rest` patterns aren't bound as individual elements here
/// (a rest binding like `rest @ ..` would need `Slice(T)` itself, which
/// is structurally distinct and out of Stage 1 scope). Operation.
fn bind_slice(
    s: &syn::PatSlice,
    matched_type: &CanonicalType,
    ctx: &InferContext<'_>,
    out: &mut Vec<(String, CanonicalType)>,
) {
    let elem_type = match matched_type {
        CanonicalType::Slice(inner) => (**inner).clone(),
        _ => CanonicalType::Opaque,
    };
    for elem in &s.elems {
        if matches!(elem, syn::Pat::Rest(_)) {
            continue;
        }
        collect(elem, &elem_type, ctx, out);
    }
}

/// `Pat::Or(a | b | c)` — in valid Rust all branches bind the same
/// names, so recording the first branch is equivalent. Operation.
fn bind_or_first(
    o: &syn::PatOr,
    matched_type: &CanonicalType,
    ctx: &InferContext<'_>,
    out: &mut Vec<(String, CanonicalType)>,
) {
    let Some(first) = o.cases.first() else {
        return;
    };
    collect(first, matched_type, ctx, out);
}
