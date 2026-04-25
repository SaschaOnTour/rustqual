//! Type-alias expansion for the resolver.
//!
//! Sibling of `resolve.rs`: shares `resolve_type_with_depth` for
//! recursion but factors out the alias-specific concerns
//! (use-site-arg pre-resolution, scope swap to the alias's decl-site,
//! param-ident interception in the body).
//!
//! Use-site flow for `Wrap<Session>` where `domain::type Wrap<T> =
//! Arc<T>`:
//!   1. `expand_alias` pre-resolves each use-site arg in the current
//!      `ResolveContext` so `Session` canonicalises against the
//!      use-site's imports.
//!   2. The context is rebuilt against the alias's decl-site
//!      `FileScope` so symbols inside the body resolve against
//!      `domain`.
//!   3. `lookup_alias_param`, invoked from `dispatch_type`'s
//!      `Type::Path` arm, intercepts naked param idents in the body
//!      and returns the canonical from step 1.

use super::super::local_symbols::FileScope;
use super::canonical::CanonicalType;
use super::resolve::{generic_type_arg, resolve_type_with_depth, ResolveContext};
use super::workspace_index::AliasDef;
use std::collections::HashMap;

/// Expand `alias` at use-site `path`. Use-site generic args are
/// pre-resolved to canonical types in the *current* scope, then the
/// body is resolved against the alias's own decl-site scope with the
/// param-name → canonical map intercepting naked param idents in
/// `dispatch_type`. Falls back to the use-site scope when
/// `workspace_files` lacks an entry for `decl_file` (legacy /
/// unit-test paths). Operation: build subs + scope swap + recurse.
pub(super) fn expand_alias(
    alias: &AliasDef,
    use_site: &syn::Path,
    ctx: &ResolveContext<'_>,
    depth: u8,
) -> CanonicalType {
    let subs = resolve_alias_param_subs(&alias.params, use_site, ctx, depth);
    let decl_file = ctx
        .workspace_files
        .and_then(|files| files.get(&alias.decl_file));
    let (file, mod_stack): (&FileScope<'_>, &[String]) = match decl_file {
        Some(f) => (f, &alias.decl_mod_stack),
        None => (ctx.file, ctx.mod_stack),
    };
    let alias_ctx = ResolveContext {
        file,
        mod_stack,
        type_aliases: ctx.type_aliases,
        transparent_wrappers: ctx.transparent_wrappers,
        workspace_files: ctx.workspace_files,
        alias_param_subs: Some(&subs),
    };
    resolve_type_with_depth(&alias.target, &alias_ctx, depth)
}

/// When inside an alias body, return the pre-resolved use-site type
/// for a bare param ident (`T`, `Output`, …). Multi-segment paths and
/// paths with arguments aren't params and pass through. Operation.
pub(super) fn lookup_alias_param(
    tp: &syn::TypePath,
    ctx: &ResolveContext<'_>,
) -> Option<CanonicalType> {
    let subs = ctx.alias_param_subs?;
    if tp.qself.is_some() || tp.path.segments.len() != 1 {
        return None;
    }
    let seg = &tp.path.segments[0];
    if !matches!(seg.arguments, syn::PathArguments::None) {
        return None;
    }
    subs.get(&seg.ident.to_string()).cloned()
}

/// Pre-resolve each use-site generic argument to a canonical type in
/// the use-site scope, keyed by alias param name. Empty when arg
/// counts disagree — the alias body's unresolved params then fall
/// through `dispatch_type` and resolve via decl-site lookup, mirroring
/// pre-Stage-3 behaviour. Operation.
fn resolve_alias_param_subs(
    params: &[String],
    use_site: &syn::Path,
    ctx: &ResolveContext<'_>,
    depth: u8,
) -> HashMap<String, CanonicalType> {
    let mut out = HashMap::new();
    if params.is_empty() {
        return out;
    }
    let Some(last) = use_site.segments.last() else {
        return out;
    };
    let args: Vec<&syn::Type> = (0..)
        .map_while(|i| generic_type_arg(&last.arguments, i))
        .collect();
    if args.len() != params.len() {
        return out;
    }
    for (name, arg) in params.iter().zip(args) {
        out.insert(name.clone(), resolve_type_with_depth(arg, ctx, depth));
    }
    out
}
