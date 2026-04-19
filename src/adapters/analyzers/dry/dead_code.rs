use std::collections::HashSet;

use syn::visit::Visit;

use super::{
    has_allow_dead_code, has_cfg_test, has_test_attr, qualify_name, DeclaredFunction, FileVisitor,
};

// ── DeclaredFnCollector (for dead code) ─────────────────────────

/// AST visitor that collects all declared function/method names with metadata.
pub(crate) struct DeclaredFnCollector {
    pub(crate) file: String,
    pub(crate) functions: Vec<DeclaredFunction>,
    in_test: bool,
    parent_type: Option<String>,
    is_trait_impl: bool,
}

impl DeclaredFnCollector {
    pub(crate) fn new() -> Self {
        Self {
            file: String::new(),
            functions: Vec::new(),
            in_test: false,
            parent_type: None,
            is_trait_impl: false,
        }
    }
}

impl FileVisitor for DeclaredFnCollector {
    fn reset_for_file(&mut self, file_path: &str) {
        self.file = file_path.to_string();
        self.in_test = false;
        self.parent_type = None;
        self.is_trait_impl = false;
    }
}

impl<'ast> Visit<'ast> for DeclaredFnCollector {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let name = node.sig.ident.to_string();
        let line = node.sig.ident.span().start().line;
        self.functions.push(DeclaredFunction {
            qualified_name: qualify_name(&self.parent_type, &name),
            is_main: name == "main",
            is_test: self.in_test || has_test_attr(&node.attrs) || has_cfg_test(&node.attrs),
            is_trait_impl: false,
            has_allow_dead_code: has_allow_dead_code(&node.attrs),
            is_api: false,
            name,
            file: self.file.clone(),
            line,
        });
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let prev_parent = self.parent_type.take();
        let prev_is_trait = self.is_trait_impl;
        let prev_in_test = self.in_test;

        if has_cfg_test(&node.attrs) {
            self.in_test = true;
        }

        self.is_trait_impl = node.trait_.is_some();
        if let syn::Type::Path(tp) = &*node.self_ty {
            if let Some(seg) = tp.path.segments.last() {
                self.parent_type = Some(seg.ident.to_string());
            }
        }

        syn::visit::visit_item_impl(self, node);

        self.parent_type = prev_parent;
        self.is_trait_impl = prev_is_trait;
        self.in_test = prev_in_test;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let name = node.sig.ident.to_string();
        let line = node.sig.ident.span().start().line;
        self.functions.push(DeclaredFunction {
            qualified_name: qualify_name(&self.parent_type, &name),
            is_main: false,
            is_test: self.in_test || has_test_attr(&node.attrs) || has_cfg_test(&node.attrs),
            is_trait_impl: self.is_trait_impl,
            has_allow_dead_code: has_allow_dead_code(&node.attrs),
            is_api: false,
            name,
            file: self.file.clone(),
            line,
        });
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        if node.default.is_some() {
            let name = node.sig.ident.to_string();
            let line = node.sig.ident.span().start().line;
            self.functions.push(DeclaredFunction {
                qualified_name: qualify_name(&self.parent_type, &name),
                is_main: false,
                is_test: self.in_test,
                is_trait_impl: true,
                has_allow_dead_code: false,
                is_api: false,
                name,
                file: self.file.clone(),
                line,
            });
        }
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let prev_in_test = self.in_test;
        if has_cfg_test(&node.attrs) {
            self.in_test = true;
        }
        syn::visit::visit_item_mod(self, node);
        self.in_test = prev_in_test;
    }
}

// ── Result types ────────────────────────────────────────────────

/// A warning about a potentially dead (unused) function.
#[derive(Debug, Clone)]
pub struct DeadCodeWarning {
    pub function_name: String,
    pub qualified_name: String,
    pub file: String,
    pub line: usize,
    pub kind: DeadCodeKind,
    pub suggestion: String,
}

/// Classification of dead code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeadCodeKind {
    /// Function is never called from anywhere (production or test).
    Uncalled,
    /// Function is only called from `#[cfg(test)]` code, not production.
    TestOnly,
}

// ── Detection API ───────────────────────────────────────────────

/// Detect dead code across parsed files.
/// Integration: orchestrates declaration collection, call collection, and finding.
/// Note: the `detect_dead_code` config flag is checked by the pipeline caller.
pub fn detect_dead_code(
    parsed: &[(String, String, syn::File)],
    config: &crate::config::Config,
    api_lines: &std::collections::HashMap<String, std::collections::HashSet<usize>>,
) -> Vec<DeadCodeWarning> {
    let cfg_test_files = collect_cfg_test_file_paths(parsed);
    let declared = super::collect_declared_functions(parsed);
    let mut declared = mark_cfg_test_declarations(declared, &cfg_test_files);
    mark_api_declarations(&mut declared, api_lines);
    let (prod_calls, test_calls) = collect_all_calls(parsed, &cfg_test_files);
    let uncalled = find_uncalled(&declared, &prod_calls, &test_calls, config);
    let test_only = find_test_only(&declared, &prod_calls, &test_calls, config);
    merge_warnings(uncalled, test_only)
}

/// Mark functions that have a `// qual:api` annotation within the annotation window.
/// Operation: iterates declarations checking line proximity to API markers.
pub(crate) fn mark_api_declarations(
    declared: &mut [super::DeclaredFunction],
    api_lines: &std::collections::HashMap<String, std::collections::HashSet<usize>>,
) {
    declared.iter_mut().for_each(|d| {
        if let Some(lines) = api_lines.get(&d.file) {
            if crate::findings::has_annotation_in_window(lines, d.line) {
                d.is_api = true;
            }
        }
    });
}

/// Merge warning lists into one.
/// Trivial: concatenation.
fn merge_warnings(
    mut uncalled: Vec<DeadCodeWarning>,
    test_only: Vec<DeadCodeWarning>,
) -> Vec<DeadCodeWarning> {
    uncalled.extend(test_only);
    uncalled
}

pub(crate) use super::cfg_test_detection::collect_cfg_test_file_paths;

/// Mark declared functions from cfg-test files as test code.
/// Trivial: iteration + field mutation.
fn mark_cfg_test_declarations(
    mut declared: Vec<super::DeclaredFunction>,
    cfg_test_files: &HashSet<String>,
) -> Vec<super::DeclaredFunction> {
    declared.iter_mut().for_each(|d| {
        if cfg_test_files.contains(&d.file) {
            d.is_test = true;
        }
    });
    declared
}

// Call target collection is in super::call_targets.
pub(crate) use super::call_targets::collect_all_calls;

// ── Finding logic ───────────────────────────────────────────────

/// Find functions that are never called from anywhere.
/// Operation: set logic + filtering, no own calls.
fn find_uncalled(
    declared: &[DeclaredFunction],
    prod_calls: &HashSet<String>,
    test_calls: &HashSet<String>,
    config: &crate::config::Config,
) -> Vec<DeadCodeWarning> {
    declared
        .iter()
        .filter(|d| !should_exclude(d, config))
        .filter(|d| !prod_calls.contains(&d.name) && !test_calls.contains(&d.name))
        .filter(|d| {
            !prod_calls.contains(&d.qualified_name) && !test_calls.contains(&d.qualified_name)
        })
        .map(|d| DeadCodeWarning {
            function_name: d.name.clone(),
            qualified_name: d.qualified_name.clone(),
            file: d.file.clone(),
            line: d.line,
            kind: DeadCodeKind::Uncalled,
            suggestion: "never called; consider removing".to_string(),
        })
        .collect()
}

/// Find functions that are only called from test code.
/// Operation: set logic + filtering, no own calls.
fn find_test_only(
    declared: &[DeclaredFunction],
    prod_calls: &HashSet<String>,
    test_calls: &HashSet<String>,
    config: &crate::config::Config,
) -> Vec<DeadCodeWarning> {
    declared
        .iter()
        .filter(|d| !should_exclude(d, config))
        // Must be called from tests but NOT from production
        .filter(|d| {
            let called_from_tests =
                test_calls.contains(&d.name) || test_calls.contains(&d.qualified_name);
            let called_from_prod =
                prod_calls.contains(&d.name) || prod_calls.contains(&d.qualified_name);
            called_from_tests && !called_from_prod
        })
        .map(|d| DeadCodeWarning {
            function_name: d.name.clone(),
            qualified_name: d.qualified_name.clone(),
            file: d.file.clone(),
            line: d.line,
            kind: DeadCodeKind::TestOnly,
            suggestion: "only called from test code; consider moving to test module".to_string(),
        })
        .collect()
}

/// Check if a declared function should be excluded from dead code analysis.
/// Operation: boolean logic combining multiple exclusion criteria.
/// The `is_ignored_function` call is hidden in a closure (lenient mode).
fn should_exclude(d: &DeclaredFunction, config: &crate::config::Config) -> bool {
    let is_ignored = |name: &str| config.is_ignored_function(name);
    d.is_main
        || d.is_test
        || d.is_trait_impl
        || d.has_allow_dead_code
        || d.is_api
        || is_ignored(&d.name)
}
