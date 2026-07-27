//! Path canonicalisation for call targets.

use super::super::bindings::{normalize_alias_expansion, CanonScope};
use super::super::local_symbols::scope_for_local;
use super::{bare, CanonicalCallCollector};
use crate::adapters::analyzers::architecture::forbidden_rule::{
    file_to_module_segments, resolve_to_crate_absolute_in,
};
use crate::adapters::shared::use_tree::AliasTarget;

impl<'a> CanonicalCallCollector<'a> {
    /// Resolve `Q::method(...)` where `Q` is a generic type param with
    /// trait bound(s) to the trait-method anchor canonicals. Returns
    /// `None` when the first segment isn't a known generic param, or
    /// when the path is an explicit absolute path (`::Q::method(...)`
    /// is the caller's disambiguation away from in-scope generics —
    /// gated centrally via `matched_generic_param`), or when the
    /// param has bounds we couldn't canonicalise. Multiple bounds =>
    /// multiple anchors (over-approximation). Operation.
    pub(super) fn canonicalise_generic_param_path(
        &self,
        segments: &[String],
        leading_colon_set: bool,
    ) -> Option<Vec<String>> {
        if segments.len() < 2 {
            return None;
        }
        let info = super::super::signature_params::matched_generic_param(
            segments,
            leading_colon_set,
            &self.generic_params,
        )?;
        let method_tail = &segments[1..];
        let canonicals: Vec<String> = info
            .bounds
            .iter()
            .map(|bound| {
                let mut full = bound.clone();
                full.extend_from_slice(method_tail);
                full.join("::")
            })
            .collect();
        Some(canonicals)
    }

    /// Turn a path-segment list into the canonical String used for all
    /// call-target comparisons in the call-parity check.
    /// `leading_colon_set` reflects `syn::Path.leading_colon` —
    /// `::Foo::bar()` is Rust 2018+ extern-root syntax that explicitly
    /// disambiguates AWAY from workspace symbols, so it short-circuits
    /// straight to `<bare>:` without consulting the alias / local /
    /// crate-root resolvers. Mirrors the same gate
    /// `canonicalise_workspace_path` enforces for `Option`-returning
    /// type canonicalisation. Integration: each branch delegates to a
    /// dedicated helper.
    pub(super) fn canonicalise_path(&self, segments: &[String], leading_colon_set: bool) -> String {
        if segments.is_empty() {
            return String::new();
        }
        // Extern-root path (`::Foo::bar`) — workspace canonicalisation
        // does not apply. Without this gate, a same-named workspace
        // symbol would produce a false `crate::...::Foo::bar` edge.
        if leading_colon_set {
            return bare(&segments.join("::"));
        }
        if segments[0] == "Self" {
            return self.canonicalise_self_path(segments);
        }
        if matches!(segments[0].as_str(), "crate" | "self" | "super") {
            return self.canonicalise_keyword_path(segments);
        }
        if let Some(canonical) = self.canonicalise_alias_path(segments) {
            return canonical;
        }
        if let Some(canonical) = self.canonicalise_local_symbol_path(segments) {
            return canonical;
        }
        // Rust 2018+ absolute call: `app::foo()` without `use` is the
        // crate-root `app` module, equivalent to `crate::app::foo()`.
        // If `app` is a known workspace root module, prepend `crate::`
        // so the canonical matches graph nodes.
        if self.file.crate_root_modules.contains(&segments[0]) {
            let mut full = vec!["crate".to_string()];
            full.extend_from_slice(segments);
            return full.join("::");
        }
        // Unknown path (external crate, stdlib, or not imported) → bare.
        bare(&segments.join("::"))
    }

    /// `Self::method` — substitute the enclosing impl's canonical
    /// self-type for `Self`. Falls back to `<bare>:` when we're not
    /// inside an impl. Operation.
    pub(super) fn canonicalise_self_path(&self, segments: &[String]) -> String {
        if let Some(self_canonical) = &self.self_type_canonical {
            let mut full = self_canonical.clone();
            full.extend_from_slice(&segments[1..]);
            return full.join("::");
        }
        bare(&segments.join("::"))
    }

    pub(super) fn canonicalise_keyword_path(&self, segments: &[String]) -> String {
        if let Some(resolved) =
            resolve_to_crate_absolute_in(self.file.path, self.mod_stack, segments)
        {
            let mut full = vec!["crate".to_string()];
            full.extend(resolved);
            return full.join("::");
        }
        bare(&segments.join("::"))
    }

    /// First segment hits a `use` alias visible at the current
    /// `mod_stack`. The alias path is then re-normalised (it may
    /// itself reference `self::`/`super::super::` or a Rust-2018 crate-root
    /// module). Returns `None` when no alias matches.
    pub(super) fn canonicalise_alias_path(&self, segments: &[String]) -> Option<String> {
        let alias = self.lookup_alias_at_scope(&segments[0])?;
        let mut full = alias.segments.to_vec();
        full.extend_from_slice(&segments[1..]);
        let scope = CanonScope {
            file: self.file,
            mod_stack: self.mod_stack,
            reexports: self.reexports,
        };
        let normalized = normalize_alias_expansion(full, alias.absolute_root, &scope)?;
        Some(normalized.join("::"))
    }

    /// Look up `name` in the alias map for exactly the current
    /// `mod_stack`. Falls back to the flat top-level `alias_map` for
    /// legacy callers that don't populate `aliases_per_scope`.
    pub(super) fn lookup_alias_at_scope(&self, name: &str) -> Option<&AliasTarget> {
        if let Some(map) = self.file.aliases_per_scope.get(self.mod_stack) {
            return map.get(name);
        }
        self.file.alias_map.get(name)
    }

    /// Same-file fallback: first segment is declared in this file at
    /// exactly the current `mod_stack`. Returns `None` when the name
    /// isn't in `local_symbols` or its declaration is in a different
    /// scope, letting the caller fall through to crate-root resolution.
    pub(super) fn canonicalise_local_symbol_path(&self, segments: &[String]) -> Option<String> {
        if !self.file.local_symbols.contains(&segments[0]) {
            return None;
        }
        let mod_path = scope_for_local(self.file.local_decl_scopes, &segments[0], self.mod_stack)?;
        let mut full = vec!["crate".to_string()];
        full.extend(file_to_module_segments(self.file.path));
        full.extend(mod_path.iter().cloned());
        full.extend_from_slice(segments);
        Some(full.join("::"))
    }
}
