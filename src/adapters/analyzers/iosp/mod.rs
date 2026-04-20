pub(crate) mod classify;
pub(crate) mod scope;
pub mod types;
pub(crate) mod visitor;

pub use classify::classify_function;
use syn::{File, ImplItem, Item, ItemFn, TraitItem};
pub use types::*;

use crate::config::Config;
use scope::ProjectScope;

use classify::extract_type_name;

/// Extract simple type names from function parameters for receiver type resolution.
/// Operation: iteration + pattern matching on type AST, no own calls.
fn extract_param_types(sig: &syn::Signature) -> std::collections::HashMap<String, String> {
    sig.inputs
        .iter()
        .filter_map(|arg| {
            if let syn::FnArg::Typed(pt) = arg {
                let name = if let syn::Pat::Ident(pi) = &*pt.pat {
                    pi.ident.to_string()
                } else {
                    return None;
                };
                extract_simple_type(&pt.ty).map(|t| (name, t))
            } else {
                None
            }
        })
        .collect()
}

/// Extract the simple type name from a type, unwrapping references and mutability.
/// Operation: recursive pattern matching, no own calls.
fn extract_simple_type(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Reference(r) => extract_simple_type(&r.elem),
        syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

/// Count non-self parameters in a function signature.
/// Operation: simple iteration + filtering logic.
fn count_non_self_params(sig: &syn::Signature) -> usize {
    sig.inputs
        .iter()
        .filter(|arg| matches!(arg, syn::FnArg::Typed(_)))
        .count()
}

/// Construct a FunctionAnalysis with pre-computed qualified_name and severity.
/// Operation: string formatting + severity computation logic, no own calls.
// qual:allow(srp) reason: "factory function — parameters map 1:1 to struct fields"
fn build_function_analysis(
    name: String,
    file_path: &str,
    line: usize,
    classification: Classification,
    parent_type: Option<String>,
    complexity: Option<ComplexityMetrics>,
    own_calls: Vec<String>,
) -> FunctionAnalysis {
    let qualified_name = parent_type
        .as_ref()
        .map(|parent| format!("{parent}::{name}"))
        .unwrap_or_else(|| name.clone());
    let severity_of = |c: &Classification| compute_severity(c);
    let severity = severity_of(&classification);
    let effort_score = if let Classification::Violation {
        logic_locations,
        call_locations,
        ..
    } = &classification
    {
        let nesting = complexity.as_ref().map_or(0, |c| c.max_nesting);
        Some(
            logic_locations.len() as f64 * EFFORT_LOGIC_WEIGHT
                + call_locations.len() as f64 * EFFORT_CALL_WEIGHT
                + nesting as f64 * EFFORT_NESTING_WEIGHT,
        )
    } else {
        None
    };
    FunctionAnalysis {
        name,
        file: file_path.to_string(),
        line,
        classification,
        parent_type,
        suppressed: false,
        complexity,
        qualified_name,
        severity,
        cognitive_warning: false,
        cyclomatic_warning: false,
        nesting_depth_warning: false,
        function_length_warning: false,
        unsafe_warning: false,
        error_handling_warning: false,
        complexity_suppressed: false,
        own_calls,
        parameter_count: 0,
        is_trait_impl: false,
        is_test: false,
        effort_score,
    }
}

/// Top-level file analyzer.
// qual:allow(srp) reason: "facade struct — analyze_file/analyze_mod delegate to methods that access fields"
pub struct Analyzer<'a> {
    config: &'a Config,
    scope: &'a ProjectScope,
    cfg_test_files: Option<&'a std::collections::HashSet<String>>,
}

impl<'a> Analyzer<'a> {
    pub fn new(config: &'a Config, scope: &'a ProjectScope) -> Self {
        Self {
            config,
            scope,
            cfg_test_files: None,
        }
    }

    /// Attach the set of cfg-test-reachable file paths. Functions in files
    /// listed here start analysis with `in_test = true`, so helper
    /// functions inside companion test files (not annotated `#[test]`
    /// directly) are still recognised as test code.
    pub fn with_cfg_test_files(
        mut self,
        cfg_test_files: &'a std::collections::HashSet<String>,
    ) -> Self {
        self.cfg_test_files = Some(cfg_test_files);
        self
    }

    /// Analyze all functions in a parsed syn::File.
    /// Trivial: iterator chain with logic in closures (lenient).
    pub fn analyze_file(&self, file: &File, file_path: &str) -> Vec<FunctionAnalysis> {
        let file_in_test = self.cfg_test_files.is_some_and(|s| s.contains(file_path));
        file.items
            .iter()
            .flat_map(|item| match item {
                Item::Fn(f) => self
                    .analyze_item_fn(f, file_path, None, file_in_test)
                    .into_iter()
                    .collect::<Vec<_>>(),
                Item::Impl(i) => {
                    let test = crate::adapters::shared::cfg_test::has_cfg_test(&i.attrs);
                    self.analyze_impl(i, file_path, test)
                }
                Item::Trait(t) => self.analyze_trait(t, file_path, false),
                Item::Mod(m) => self.analyze_mod(m, file_path, false),
                _ => vec![],
            })
            .collect()
    }

    /// Build a FunctionAnalysis from classification results.
    /// Integration: orchestrates classify_function, compute_severity, build_function_analysis.
    fn classify_and_build(
        &self,
        name: String,
        file_path: &str,
        body: &syn::Block,
        parent_type: Option<String>,
        sig: &syn::Signature,
    ) -> FunctionAnalysis {
        let type_ctx = (parent_type.as_deref(), sig);
        let (classification, complexity, own_calls) =
            classify_function(body, self.config, self.scope, &name, type_ctx);
        let line = sig.ident.span().start().line;
        build_function_analysis(
            name,
            file_path,
            line,
            classification,
            parent_type,
            complexity,
            own_calls,
        )
    }

    /// Analyze a single function item.
    /// Integration: orchestrates is_ignored_function check + classify_and_build (in closure).
    fn analyze_item_fn(
        &self,
        item_fn: &ItemFn,
        file_path: &str,
        parent_type: Option<String>,
        in_test: bool,
    ) -> Option<FunctionAnalysis> {
        let name = item_fn.sig.ident.to_string();
        (!self.config.is_ignored_function(&name)).then(|| {
            let mut fa =
                self.classify_and_build(name, file_path, &item_fn.block, parent_type, &item_fn.sig);
            fa.parameter_count = count_non_self_params(&item_fn.sig);
            fa.is_test =
                in_test || crate::adapters::shared::cfg_test::has_test_attr(&item_fn.attrs);
            fa
        })
    }

    /// Analyze all methods in an impl block.
    /// Integration: orchestrates extract_type_name + per-method analysis in iterator chain.
    fn analyze_impl(
        &self,
        item_impl: &syn::ItemImpl,
        file_path: &str,
        in_test: bool,
    ) -> Vec<FunctionAnalysis> {
        let type_name = extract_type_name(item_impl);
        let trait_impl = item_impl.trait_.is_some();
        item_impl
            .items
            .iter()
            .filter_map(|impl_item| {
                if let ImplItem::Fn(method) = impl_item {
                    let name = method.sig.ident.to_string();
                    if self.config.is_ignored_function(&name) {
                        return None;
                    }
                    let mut fa = self.classify_and_build(
                        name,
                        file_path,
                        &method.block,
                        type_name.clone(),
                        &method.sig,
                    );
                    fa.parameter_count = count_non_self_params(&method.sig);
                    fa.is_trait_impl = trait_impl;
                    fa.is_test = in_test;
                    Some(fa)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Analyze default method implementations in a trait.
    /// Trivial: iterator chain.
    fn analyze_trait(
        &self,
        item_trait: &syn::ItemTrait,
        file_path: &str,
        in_test: bool,
    ) -> Vec<FunctionAnalysis> {
        let trait_name = item_trait.ident.to_string();
        item_trait
            .items
            .iter()
            .filter_map(|trait_item| {
                if let TraitItem::Fn(method) = trait_item {
                    let block = method.default.as_ref()?;
                    let name = method.sig.ident.to_string();
                    if self.config.is_ignored_function(&name) {
                        return None;
                    }
                    let mut fa = self.classify_and_build(
                        name,
                        file_path,
                        block,
                        Some(trait_name.clone()),
                        &method.sig,
                    );
                    fa.parameter_count = count_non_self_params(&method.sig);
                    fa.is_trait_impl = true;
                    fa.is_test = in_test;
                    Some(fa)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Recursively analyze inline modules.
    /// Trivial: iterator chain.
    fn analyze_mod(
        &self,
        item_mod: &syn::ItemMod,
        file_path: &str,
        in_test: bool,
    ) -> Vec<FunctionAnalysis> {
        let mod_is_test =
            in_test || crate::adapters::shared::cfg_test::has_cfg_test(&item_mod.attrs);
        item_mod
            .content
            .as_ref()
            .map(|(_, items)| {
                items
                    .iter()
                    .flat_map(|item| match item {
                        Item::Fn(f) => self
                            .analyze_item_fn(f, file_path, None, mod_is_test)
                            .into_iter()
                            .collect::<Vec<_>>(),
                        Item::Impl(i) => {
                            let test = mod_is_test
                                || crate::adapters::shared::cfg_test::has_cfg_test(&i.attrs);
                            self.analyze_impl(i, file_path, test)
                        }
                        Item::Trait(t) => self.analyze_trait(t, file_path, mod_is_test),
                        Item::Mod(m) => self.analyze_mod(m, file_path, mod_is_test),
                        _ => vec![],
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests;
