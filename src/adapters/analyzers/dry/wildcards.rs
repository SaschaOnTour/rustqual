use syn::spanned::Spanned;
use syn::visit::Visit;

use super::FileVisitor;

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
    let mut collector = WildcardCollector {
        file: String::new(),
        warnings: Vec::new(),
        in_test: false,
    };
    super::visit_all_files(parsed, &mut collector);
    collector.warnings
}

struct WildcardCollector {
    file: String,
    warnings: Vec<WildcardImportWarning>,
    in_test: bool,
}

impl FileVisitor for WildcardCollector {
    fn reset_for_file(&mut self, file_path: &str) {
        // Normalise separators once so downstream checks (e.g. "/tests/"
        // companion-file detection) work on Windows paths too.
        self.file = file_path.replace('\\', "/");
        self.in_test = false;
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
                    // Skip `use super::*` in test modules
                    if self.in_test && prefix.first().is_some_and(|p| p == "super") {
                        continue;
                    }
                    // Skip wildcard imports in files that live under a
                    // `tests/` directory — those are companion test files
                    // loaded via `#[cfg(test)] mod tests;` from a parent.
                    if self.file.contains("/tests/") {
                        continue;
                    }
                    // Skip `prelude::*` paths
                    if prefix.last().is_some_and(|p| p == "prelude") {
                        continue;
                    }
                    let path = if prefix.is_empty() {
                        "*".to_string()
                    } else {
                        format!("{}::*", prefix.join("::"))
                    };
                    self.warnings.push(WildcardImportWarning {
                        file: self.file.clone(),
                        line: node.span().start().line,
                        module_path: path,
                        suppressed: false,
                    });
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
