use std::collections::HashSet;

use syn::spanned::Spanned;
use syn::visit::Visit;

use crate::adapters::shared::file_visitor::{visit_all_files, FileVisitor};

/// A wildcard import warning (e.g. `use crate::module::*`).
#[derive(Debug, Clone)]
pub struct WildcardImportWarning {
    /// File containing the wildcard import.
    pub file: String,
    /// Line number of the `use` statement.
    pub line: usize,
    /// Full module path of the wildcard import (e.g. `crate::adapters::analyzers::iosp::*`).
    pub module_path: String,
    /// Whether this warning is suppressed via `// qual:allow(dry)`.
    pub suppressed: bool,
}

/// Detect wildcard imports in parsed files.
/// Trivial: creates visitor and delegates to visit_all_files.
pub fn detect_wildcard_imports(
    parsed: &[(String, String, syn::File)],
) -> Vec<WildcardImportWarning> {
    let cfg_test_files =
        crate::adapters::shared::cfg_test_files::collect_cfg_test_file_paths(parsed);
    let mut collector = WildcardCollector {
        cfg_test_files,
        file: String::new(),
        warnings: Vec::new(),
        in_test: false,
        file_is_test: false,
    };
    visit_all_files(parsed, &mut collector);
    collector.warnings
}

struct WildcardCollector {
    cfg_test_files: HashSet<String>,
    file: String,
    warnings: Vec<WildcardImportWarning>,
    in_test: bool,
    file_is_test: bool,
}

impl FileVisitor for WildcardCollector {
    fn reset_for_file(&mut self, file_path: &str) {
        // Whole-file test classification comes from the authoritative
        // cfg-test file set (checked against the raw path, matching the
        // set's keys). `self.file` is separately separator-normalised so
        // the emitted warning path is stable on Windows.
        self.file_is_test = self.cfg_test_files.contains(file_path);
        self.file = file_path.replace('\\', "/");
        self.in_test = false;
    }
}

impl WildcardCollector {
    /// Whether a glob import under `prefix` is exempt: bare `use super::*` in a
    /// test module, any import in a test file, or a `prelude` wildcard.
    /// Operation: boolean guard logic, no own calls.
    fn skip_glob(&self, prefix: &[String]) -> bool {
        (self.in_test && prefix == ["super"])
            || self.file_is_test
            || prefix.iter().any(|p| p == "prelude")
    }

    /// Record a wildcard-import warning for `prefix` at `line`.
    /// Operation: path formatting + push, no own calls.
    fn push_glob_warning(&mut self, prefix: &[String], line: usize) {
        let module_path = if prefix.is_empty() {
            "*".to_string()
        } else {
            format!("{}::*", prefix.join("::"))
        };
        self.warnings.push(WildcardImportWarning {
            file: self.file.clone(),
            line,
            module_path,
            suppressed: false,
        });
    }
}

impl<'ast> Visit<'ast> for WildcardCollector {
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        // Skip `pub use` / `pub(crate) use` re-exports — they are an API design pattern, not lazy imports.
        if !matches!(node.vis, syn::Visibility::Inherited) {
            return;
        }
        // Walk the use tree iteratively to find glob imports.
        let mut stack: Vec<(Vec<String>, &syn::UseTree)> = vec![(vec![], &node.tree)];
        while let Some((prefix, tree)) = stack.pop() {
            match tree {
                syn::UseTree::Path(p) => {
                    let mut new_prefix = prefix;
                    new_prefix.push(p.ident.to_string());
                    stack.push((new_prefix, &p.tree));
                }
                syn::UseTree::Glob(_) => {
                    let record = !self.skip_glob(&prefix);
                    record.then(|| self.push_glob_warning(&prefix, node.span().start().line));
                }
                syn::UseTree::Group(g) => {
                    for item in &g.items {
                        stack.push((prefix.clone(), item));
                    }
                }
                // Name and Rename are not globs, skip
                _ => {}
            }
        }
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let prev = self.in_test;
        if super::has_cfg_test(&node.attrs) {
            self.in_test = true;
        }
        syn::visit::visit_item_mod(self, node);
        self.in_test = prev;
    }
}
