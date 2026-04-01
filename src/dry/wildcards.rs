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
    /// Full module path of the wildcard import (e.g. `crate::analyzer::*`).
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
        self.file = file_path.to_string();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(code: &str) -> Vec<(String, String, syn::File)> {
        let syntax = syn::parse_file(code).expect("parse failed");
        vec![("test.rs".to_string(), code.to_string(), syntax)]
    }

    #[test]
    fn test_detects_simple_glob() {
        let parsed = parse("use crate::module::*;");
        let warnings = detect_wildcard_imports(&parsed);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].module_path, "crate::module::*");
        assert_eq!(warnings[0].file, "test.rs");
    }

    #[test]
    fn test_detects_nested_glob() {
        let parsed = parse("use crate::module::sub::*;");
        let warnings = detect_wildcard_imports(&parsed);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].module_path, "crate::module::sub::*");
    }

    #[test]
    fn test_no_warning_for_named_import() {
        let parsed = parse("use crate::module::Foo;");
        let warnings = detect_wildcard_imports(&parsed);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_no_warning_for_group_import() {
        let parsed = parse("use crate::module::{Foo, Bar};");
        let warnings = detect_wildcard_imports(&parsed);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_glob_inside_group() {
        let parsed = parse("use crate::{module::*, other::Foo};");
        let warnings = detect_wildcard_imports(&parsed);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].module_path, "crate::module::*");
    }

    #[test]
    fn test_excludes_super_in_test() {
        let code = r#"
            #[cfg(test)]
            mod tests {
                use super::*;
            }
        "#;
        let parsed = parse(code);
        let warnings = detect_wildcard_imports(&parsed);
        assert!(
            warnings.is_empty(),
            "use super::* in #[cfg(test)] should be excluded"
        );
    }

    #[test]
    fn test_super_glob_outside_test_flagged() {
        let code = "use super::*;";
        let parsed = parse(code);
        let warnings = detect_wildcard_imports(&parsed);
        assert_eq!(
            warnings.len(),
            1,
            "use super::* outside test should be flagged"
        );
        assert_eq!(warnings[0].module_path, "super::*");
    }

    #[test]
    fn test_excludes_prelude_glob() {
        let parsed = parse("use std::prelude::*;");
        let warnings = detect_wildcard_imports(&parsed);
        assert!(warnings.is_empty(), "prelude::* should be excluded");
    }

    #[test]
    fn test_excludes_custom_prelude_glob() {
        let parsed = parse("use crate::prelude::*;");
        let warnings = detect_wildcard_imports(&parsed);
        assert!(warnings.is_empty(), "crate::prelude::* should be excluded");
    }

    #[test]
    fn test_external_glob_detected() {
        let parsed = parse("use std::collections::*;");
        let warnings = detect_wildcard_imports(&parsed);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].module_path, "std::collections::*");
    }

    #[test]
    fn test_multiple_globs_in_file() {
        let code = "use crate::a::*;\nuse crate::b::*;\nuse crate::c::Foo;";
        let parsed = parse(code);
        let warnings = detect_wildcard_imports(&parsed);
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn test_multiple_files() {
        let code1 = "use crate::module::*;";
        let code2 = "use crate::other::*;";
        let syntax1 = syn::parse_file(code1).unwrap();
        let syntax2 = syn::parse_file(code2).unwrap();
        let parsed = vec![
            ("a.rs".to_string(), code1.to_string(), syntax1),
            ("b.rs".to_string(), code2.to_string(), syntax2),
        ];
        let warnings = detect_wildcard_imports(&parsed);
        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0].file, "a.rs");
        assert_eq!(warnings[1].file, "b.rs");
    }

    #[test]
    fn test_glob_line_number() {
        let code = "fn foo() {}\nuse crate::module::*;\nfn bar() {}";
        let parsed = parse(code);
        let warnings = detect_wildcard_imports(&parsed);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 2);
    }

    #[test]
    fn test_empty_file() {
        let parsed = parse("");
        let warnings = detect_wildcard_imports(&parsed);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_not_suppressed_by_default() {
        let parsed = parse("use crate::module::*;");
        let warnings = detect_wildcard_imports(&parsed);
        assert!(!warnings[0].suppressed);
    }

    #[test]
    fn test_pub_use_reexport_excluded() {
        let parsed = parse("pub use crate::module::*;");
        let warnings = detect_wildcard_imports(&parsed);
        assert!(warnings.is_empty(), "pub use re-exports should be excluded");
    }

    #[test]
    fn test_pub_crate_use_reexport_excluded() {
        let parsed = parse("pub(crate) use crate::module::*;");
        let warnings = detect_wildcard_imports(&parsed);
        assert!(
            warnings.is_empty(),
            "pub(crate) use re-exports should be excluded"
        );
    }
}
