use std::collections::HashSet;
use std::path::Path;

use syn::visit::Visit;

use super::DeclaredFunction;

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
    let declared = mark_cfg_test_declarations(declared, &cfg_test_files);
    let declared = mark_api_declarations(declared, api_lines);
    let (prod_calls, test_calls) = collect_all_calls(parsed, &cfg_test_files);
    let uncalled = find_uncalled(&declared, &prod_calls, &test_calls, config);
    let test_only = find_test_only(&declared, &prod_calls, &test_calls, config);
    merge_warnings(uncalled, test_only)
}

/// Mark functions that have a `// qual:api` annotation on the preceding line.
/// Operation: iterates declarations checking line proximity to API markers.
fn mark_api_declarations(
    mut declared: Vec<super::DeclaredFunction>,
    api_lines: &std::collections::HashMap<String, std::collections::HashSet<usize>>,
) -> Vec<super::DeclaredFunction> {
    declared.iter_mut().for_each(|d| {
        if let Some(lines) = api_lines.get(&d.file) {
            if lines.contains(&d.line) || (d.line > 1 && lines.contains(&(d.line - 1))) {
                d.is_api = true;
            }
        }
    });
    declared
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

// ── cfg(test) module file detection ─────────────────────────────

/// Scan all parsed files for `#[cfg(test)] mod name;` (external module declarations)
/// and compute which child file paths are test-only.
/// Operation: path computation logic, no own calls. Inner helpers hidden in closures.
pub(crate) fn collect_cfg_test_file_paths(
    parsed: &[(String, String, syn::File)],
) -> HashSet<String> {
    let all_paths: HashSet<&str> = parsed.iter().map(|(p, _, _)| p.as_str()).collect();
    let is_ext_cfg_test = |m: &syn::ItemMod| m.content.is_none() && super::has_cfg_test(&m.attrs);
    let resolve = |parent_path: &str, mod_name: &str| -> Option<String> {
        let parent = Path::new(parent_path);
        let child_dir = if parent
            .file_stem()
            .is_some_and(|s| s == "mod" || s == "lib" || s == "main")
        {
            parent.parent().unwrap_or(Path::new("")).to_path_buf()
        } else {
            parent.with_extension("")
        };
        let f = child_dir
            .join(format!("{mod_name}.rs"))
            .to_string_lossy()
            .into_owned();
        let d = child_dir
            .join(mod_name)
            .join("mod.rs")
            .to_string_lossy()
            .into_owned();
        if all_paths.contains(f.as_str()) {
            Some(f)
        } else if all_paths.contains(d.as_str()) {
            Some(d)
        } else {
            None
        }
    };
    parsed
        .iter()
        .flat_map(|(path, _, file)| {
            file.items
                .iter()
                .filter_map(|item| match item {
                    syn::Item::Mod(m) if is_ext_cfg_test(m) => {
                        Some((path.as_str(), m.ident.to_string()))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .filter_map(|(parent, name)| resolve(parent, &name))
        .collect()
}

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

// ── Call target collection ──────────────────────────────────────

/// Collect all function/method call targets from all parsed files,
/// separated into production and test contexts.
/// Trivial: creates visitor and delegates via for_each closure.
pub(crate) fn collect_all_calls(
    parsed: &[(String, String, syn::File)],
    cfg_test_files: &HashSet<String>,
) -> (HashSet<String>, HashSet<String>) {
    let mut collector = CallTargetCollector {
        production_calls: HashSet::new(),
        test_calls: HashSet::new(),
        in_test: false,
    };
    parsed.iter().for_each(|(path, _, file)| {
        collector.in_test = cfg_test_files.contains(path);
        syn::visit::visit_file(&mut collector, file);
    });
    (collector.production_calls, collector.test_calls)
}

/// AST visitor that collects all function/method call targets.
struct CallTargetCollector {
    production_calls: HashSet<String>,
    test_calls: HashSet<String>,
    in_test: bool,
}

/// Insert the last path segment and qualified `Type::method` form into the target set.
fn insert_path_segments(target: &mut HashSet<String>, path: &syn::Path) {
    let segments: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    if let Some(last) = segments.last() {
        target.insert(last.clone());
    }
    if segments.len() >= 2 {
        target.insert(format!(
            "{}::{}",
            segments[segments.len() - 2],
            segments.last().unwrap()
        ));
    }
}

impl CallTargetCollector {
    /// Extract function names referenced by serde field attributes.
    /// Operation: attribute parsing logic, no own calls.
    fn extract_serde_fn_refs(attrs: &[syn::Attribute]) -> Vec<String> {
        let mut refs = Vec::new();
        let push_fn_ref = |refs: &mut Vec<String>, s: String| {
            if let Some(name) = s.rsplit("::").next() {
                refs.push(name.to_string());
            }
            if s.contains("::") {
                refs.push(s);
            }
        };
        attrs
            .iter()
            .filter(|a| a.path().is_ident("serde"))
            .for_each(|attr| {
                let _ = attr.parse_nested_meta(|meta| {
                    let is_fn_key = meta.path.is_ident("deserialize_with")
                        || meta.path.is_ident("serialize_with")
                        || meta.path.is_ident("default");
                    if is_fn_key || meta.path.is_ident("with") {
                        if let Ok(value) = meta.value() {
                            if let Ok(lit) = value.parse::<syn::LitStr>() {
                                let s = lit.value();
                                if is_fn_key {
                                    push_fn_ref(&mut refs, s);
                                } else {
                                    refs.push(format!("{s}::serialize"));
                                    refs.push(format!("{s}::deserialize"));
                                    refs.extend(["serialize".into(), "deserialize".into()]);
                                }
                            }
                        }
                    }
                    Ok(())
                });
            });
        refs
    }

    /// Extract function references from call arguments (e.g., `.for_each(some_fn)`).
    fn record_path_args(
        &mut self,
        args: &syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
    ) {
        let target = if self.in_test {
            &mut self.test_calls
        } else {
            &mut self.production_calls
        };
        args.iter().for_each(|arg| {
            if let syn::Expr::Path(p) = arg {
                insert_path_segments(target, &p.path);
            }
        });
    }
}

impl<'ast> Visit<'ast> for CallTargetCollector {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*node.func {
            let target = if self.in_test {
                &mut self.test_calls
            } else {
                &mut self.production_calls
            };
            insert_path_segments(target, &p.path);
        }
        self.record_path_args(&node.args);
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        let name = node.method.to_string();
        if self.in_test {
            self.test_calls.insert(name);
        } else {
            self.production_calls.insert(name);
        }
        self.record_path_args(&node.args);
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let prev = self.in_test;
        if super::has_cfg_test(&node.attrs) {
            self.in_test = true;
        }
        syn::visit::visit_item_mod(self, node);
        self.in_test = prev;
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let prev = self.in_test;
        if super::has_test_attr(&node.attrs) {
            self.in_test = true;
        }
        syn::visit::visit_item_fn(self, node);
        self.in_test = prev;
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        // Parse macro arguments as expressions to find embedded function calls.
        // Works for assert!(), assert_eq!(), format!(), vec![], etc.
        use syn::punctuated::Punctuated;
        if let Ok(args) = syn::parse::Parser::parse2(
            Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated,
            node.tokens.clone(),
        ) {
            args.iter()
                .for_each(|expr| syn::visit::visit_expr(self, expr));
        }
        syn::visit::visit_macro(self, node);
    }

    fn visit_field(&mut self, node: &'ast syn::Field) {
        let refs = Self::extract_serde_fn_refs(&node.attrs);
        if self.in_test {
            self.test_calls.extend(refs);
        } else {
            self.production_calls.extend(refs);
        }
        syn::visit::visit_field(self, node);
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        // Only pub/pub(crate) re-exports count as usage of the original function.
        // Private `use` imports are not re-exports; their call targets are already
        // captured via visit_expr_call when the imported name is actually called.
        if matches!(node.vis, syn::Visibility::Inherited) {
            return;
        }
        let target = if self.in_test {
            &mut self.test_calls
        } else {
            &mut self.production_calls
        };
        // Iterative UseTree walk
        let mut stack: Vec<&syn::UseTree> = vec![&node.tree];
        while let Some(tree) = stack.pop() {
            match tree {
                syn::UseTree::Name(n) => {
                    target.insert(n.ident.to_string());
                }
                syn::UseTree::Rename(r) => {
                    // Record the ORIGINAL name (r.ident), not the alias (r.rename).
                    target.insert(r.ident.to_string());
                }
                syn::UseTree::Path(p) => stack.push(&p.tree),
                syn::UseTree::Group(g) => stack.extend(&g.items),
                syn::UseTree::Glob(_) => {} // Can't enumerate; skip
            }
        }
        // No need to recurse — ItemUse has no child expressions to visit.
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn parse(code: &str) -> Vec<(String, String, syn::File)> {
        let syntax = syn::parse_file(code).expect("parse failed");
        vec![("test.rs".to_string(), code.to_string(), syntax)]
    }

    #[test]
    fn test_detect_dead_code_empty() {
        let parsed = parse("");
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_uncalled_function_detected() {
        let code = r#"
            fn called_fn() { let x = 1; }
            fn caller() { called_fn(); }
            fn never_called() { let y = 2; }
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        let uncalled: Vec<_> = warnings
            .iter()
            .filter(|w| w.kind == DeadCodeKind::Uncalled)
            .collect();
        assert!(
            uncalled.iter().any(|w| w.function_name == "never_called"),
            "never_called should be flagged as uncalled"
        );
        assert!(
            !uncalled.iter().any(|w| w.function_name == "called_fn"),
            "called_fn should not be flagged"
        );
    }

    #[test]
    fn test_called_function_not_flagged() {
        let code = r#"
            fn helper() { let x = 1; }
            fn main() { helper(); }
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "helper"),
            "called function should not be flagged"
        );
    }

    #[test]
    fn test_main_excluded_from_dead_code() {
        let code = "fn main() {}";
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "main"),
            "main should never be flagged"
        );
    }

    #[test]
    fn test_test_function_excluded() {
        let code = r#"
            #[test]
            fn test_something() { let x = 1; }
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "test_something"),
            "test functions should be excluded"
        );
    }

    #[test]
    fn test_trait_impl_excluded() {
        let code = r#"
            trait Foo { fn bar(&self); }
            struct S;
            impl Foo for S {
                fn bar(&self) {}
            }
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "bar"),
            "trait impl methods should be excluded"
        );
    }

    #[test]
    fn test_allow_dead_code_excluded() {
        let code = r#"
            #[allow(dead_code)]
            fn intentionally_unused() { let x = 1; }
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings
                .iter()
                .any(|w| w.function_name == "intentionally_unused"),
            "Functions with #[allow(dead_code)] should be excluded"
        );
    }

    #[test]
    fn test_ignored_function_excluded() {
        let code = r#"
            fn visit_expr(&self) { let x = 1; }
        "#;
        let parsed = parse(code);
        let mut config = Config::default();
        config.ignore_functions = vec!["visit_*".to_string()];
        config.compile();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "visit_expr"),
            "Ignored functions should be excluded"
        );
    }

    #[test]
    fn test_test_only_function_detected() {
        let code = r#"
            fn helper() { let x = 1; }
            fn production() { let y = 2; }
            #[cfg(test)]
            mod tests {
                use super::*;
                #[test]
                fn test_it() { helper(); }
            }
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        let test_only: Vec<_> = warnings
            .iter()
            .filter(|w| w.kind == DeadCodeKind::TestOnly)
            .collect();
        assert!(
            test_only.iter().any(|w| w.function_name == "helper"),
            "helper called only from tests should be flagged as test-only"
        );
    }

    #[test]
    fn test_dead_code_always_runs_when_called_directly() {
        // The detect_dead_code flag is checked by the pipeline caller, not by
        // detect_dead_code itself (to maintain IOSP Integration compliance).
        let code = r#"
            fn never_called() { let x = 1; }
        "#;
        let parsed = parse(code);
        let mut config = Config::default();
        config.duplicates.detect_dead_code = false;
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.is_empty(),
            "detect_dead_code runs regardless — pipeline guards the config flag"
        );
    }

    #[test]
    fn test_method_call_detected() {
        let code = r#"
            struct S;
            impl S {
                fn helper(&self) { let x = 1; }
                fn caller(&self) { self.helper(); }
            }
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "helper"),
            "Method called via self.helper() should not be flagged"
        );
    }

    #[test]
    fn test_function_reference_as_call_argument() {
        let code = r#"
            fn some_fn(x: i32) -> i32 { x + 1 }
            fn caller() {
                let items = vec![1, 2, 3];
                let _: Vec<_> = items.into_iter().map(some_fn).collect();
            }
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "some_fn"),
            "Function passed as argument to map() should be detected as called"
        );
    }

    #[test]
    fn test_function_reference_as_method_argument() {
        let code = r#"
            fn process(x: i32) { let _ = x; }
            fn caller() {
                let items = vec![1, 2, 3];
                items.iter().for_each(process);
            }
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "process"),
            "Function passed as argument to for_each() should be detected as called"
        );
    }

    #[test]
    fn test_qualified_function_reference_as_argument() {
        let code = r#"
            mod report {
                pub fn print_item(x: &i32) { let _ = x; }
            }
            fn caller() {
                let items = vec![1, 2, 3];
                items.iter().for_each(report::print_item);
            }
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "print_item"),
            "Qualified function reference (module::fn) should be detected as called"
        );
    }

    #[test]
    fn test_qualified_call_detected() {
        let code = r#"
            struct Config;
            impl Config {
                fn load() -> Self { Config }
            }
            fn main() { let c = Config::load(); }
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "load"),
            "Config::load() should be detected as called"
        );
    }

    #[test]
    fn test_pub_use_reexport_not_dead_code() {
        let code = r#"
            mod foo { pub fn do_work() { let x = 1; } }
            pub use foo::do_work;
            fn main() {}
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "do_work"),
            "pub use re-exported function should not be flagged as dead code"
        );
    }

    #[test]
    fn test_pub_use_rename_not_dead_code() {
        let code = r#"
            mod foo { pub fn do_work() { let x = 1; } }
            pub use foo::do_work as perform_work;
            fn main() {}
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "do_work"),
            "pub use rename re-export should record original name, not alias"
        );
    }

    #[test]
    fn test_pub_use_group_reexport_not_dead_code() {
        let code = r#"
            mod foo {
                pub fn bar() { let x = 1; }
                pub fn baz() { let y = 2; }
            }
            pub use foo::{bar, baz};
            fn main() {}
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "bar"),
            "grouped pub use re-export: bar should not be flagged"
        );
        assert!(
            !warnings.iter().any(|w| w.function_name == "baz"),
            "grouped pub use re-export: baz should not be flagged"
        );
    }

    #[test]
    fn test_private_use_does_not_count_as_reexport() {
        let code = r#"
            mod foo { pub fn helper() { let x = 1; } }
            use foo::helper;
            fn main() {}
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            warnings.iter().any(|w| w.function_name == "helper"),
            "private use import (no call) should still be flagged as uncalled"
        );
    }

    #[test]
    fn test_cfg_test_mod_file_not_flagged() {
        // Parent file declares `#[cfg(test)] mod helpers;` (external module)
        let parent_code = r#"
            fn production_fn() { let x = 1; }
            #[cfg(test)]
            mod helpers;
        "#;
        // Child file contains helper functions (no #[cfg(test)] at root)
        let child_code = r#"
            pub fn test_helper() { let x = 1; }
        "#;
        let parent_ast = syn::parse_file(parent_code).expect("parse parent");
        let child_ast = syn::parse_file(child_code).expect("parse child");
        let parsed = vec![
            (
                "src/mod.rs".to_string(),
                parent_code.to_string(),
                parent_ast,
            ),
            (
                "src/helpers.rs".to_string(),
                child_code.to_string(),
                child_ast,
            ),
        ];
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        // test_helper lives in a cfg(test) module file — should be excluded
        assert!(
            !warnings.iter().any(|w| w.function_name == "test_helper"),
            "Functions in #[cfg(test)] mod file should not be flagged as dead code"
        );
    }

    #[test]
    fn test_cfg_test_mod_calls_classified_as_test() {
        // Parent declares #[cfg(test)] mod helpers; externally
        let parent_code = r#"
            fn used_by_test_helpers() { let x = 1; }
            fn used_by_production() { let y = 2; }
            fn caller() { used_by_production(); }
            #[cfg(test)]
            mod helpers;
        "#;
        // Child file calls used_by_test_helpers — should be a test call
        let child_code = r#"
            pub fn test_helper() { super::used_by_test_helpers(); }
        "#;
        let parent_ast = syn::parse_file(parent_code).expect("parse parent");
        let child_ast = syn::parse_file(child_code).expect("parse child");
        let parsed = vec![
            (
                "src/lib.rs".to_string(),
                parent_code.to_string(),
                parent_ast,
            ),
            (
                "src/helpers.rs".to_string(),
                child_code.to_string(),
                child_ast,
            ),
        ];
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        // used_by_test_helpers is only called from cfg(test) file → TestOnly
        let test_only: Vec<_> = warnings
            .iter()
            .filter(|w| w.kind == DeadCodeKind::TestOnly)
            .collect();
        assert!(
            test_only
                .iter()
                .any(|w| w.function_name == "used_by_test_helpers"),
            "Function called only from cfg(test) file should be flagged as test-only"
        );
        // used_by_production is called from production code → not flagged
        assert!(
            !warnings
                .iter()
                .any(|w| w.function_name == "used_by_production"),
            "Function called from production should not be flagged"
        );
    }

    #[test]
    fn test_cfg_test_mod_dir_module() {
        // Parent declares #[cfg(test)] mod helpers; where child is helpers/mod.rs
        let parent_code = r#"
            fn prod() { let x = 1; }
            #[cfg(test)]
            mod helpers;
        "#;
        let child_code = r#"
            pub fn test_util() { let x = 1; }
        "#;
        let parent_ast = syn::parse_file(parent_code).expect("parse parent");
        let child_ast = syn::parse_file(child_code).expect("parse child");
        let parsed = vec![
            (
                "src/foo/mod.rs".to_string(),
                parent_code.to_string(),
                parent_ast,
            ),
            (
                "src/foo/helpers/mod.rs".to_string(),
                child_code.to_string(),
                child_ast,
            ),
        ];
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "test_util"),
            "Functions in #[cfg(test)] dir module (mod.rs) should not be flagged"
        );
    }

    #[test]
    fn test_cfg_test_file_path_from_non_mod_parent() {
        // Parent is foo.rs (not mod.rs) → child dir is foo/
        let parent_code = r#"
            fn prod() { let x = 1; }
            #[cfg(test)]
            mod test_utils;
        "#;
        let child_code = r#"
            pub fn helper() { let x = 1; }
        "#;
        let parent_ast = syn::parse_file(parent_code).expect("parse parent");
        let child_ast = syn::parse_file(child_code).expect("parse child");
        let parsed = vec![
            (
                "src/foo.rs".to_string(),
                parent_code.to_string(),
                parent_ast,
            ),
            (
                "src/foo/test_utils.rs".to_string(),
                child_code.to_string(),
                child_ast,
            ),
        ];
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "helper"),
            "Functions in cfg(test) child of foo.rs → foo/test_utils.rs should be excluded"
        );
    }

    #[test]
    fn test_collect_cfg_test_file_paths_basic() {
        let parent_code = r#"
            #[cfg(test)]
            mod helpers;
        "#;
        let child_code = "pub fn h() {}";
        let parent_ast = syn::parse_file(parent_code).unwrap();
        let child_ast = syn::parse_file(child_code).unwrap();
        let parsed = vec![
            (
                "src/lib.rs".to_string(),
                parent_code.to_string(),
                parent_ast,
            ),
            (
                "src/helpers.rs".to_string(),
                child_code.to_string(),
                child_ast,
            ),
        ];
        let result = collect_cfg_test_file_paths(&parsed);
        assert!(
            result.contains("src/helpers.rs"),
            "Should detect src/helpers.rs as cfg-test file"
        );
    }

    // ── Serde attribute tests ────────────────────────────────

    #[test]
    fn test_serde_deserialize_with_not_dead_code() {
        let code = r#"
            fn custom_de<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i32, D::Error> {
                let v: i32 = serde::Deserialize::deserialize(d)?;
                Ok(v)
            }
            #[derive(serde::Deserialize)]
            struct Foo {
                #[serde(deserialize_with = "custom_de")]
                value: i32,
            }
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "custom_de"),
            "Function referenced by #[serde(deserialize_with)] should not be flagged"
        );
    }

    #[test]
    fn test_serde_serialize_with_not_dead_code() {
        let code = r#"
            fn custom_ser<S: serde::Serializer>(v: &i32, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_i32(*v)
            }
            #[derive(serde::Serialize)]
            struct Foo {
                #[serde(serialize_with = "custom_ser")]
                value: i32,
            }
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "custom_ser"),
            "Function referenced by #[serde(serialize_with)] should not be flagged"
        );
    }

    #[test]
    fn test_serde_default_fn_not_dead_code() {
        let code = r#"
            fn default_val() -> i32 { 42 }
            #[derive(serde::Deserialize)]
            struct Foo {
                #[serde(default = "default_val")]
                value: i32,
            }
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "default_val"),
            "Function referenced by #[serde(default = \"fn\")] should not be flagged"
        );
    }

    #[test]
    fn test_serde_qualified_path_not_dead_code() {
        let code = r#"
            mod helpers {
                pub fn custom_de<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i32, D::Error> {
                    let v: i32 = serde::Deserialize::deserialize(d)?;
                    Ok(v)
                }
            }
            #[derive(serde::Deserialize)]
            struct Foo {
                #[serde(deserialize_with = "helpers::custom_de")]
                value: i32,
            }
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "custom_de"),
            "Qualified serde fn ref (helpers::custom_de) should not be flagged"
        );
    }

    #[test]
    fn test_serde_with_module_not_dead_code() {
        let code = r#"
            mod my_format {
                pub fn serialize<S: serde::Serializer>(_v: &i32, s: S) -> Result<S::Ok, S::Error> {
                    s.serialize_i32(0)
                }
                pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i32, D::Error> {
                    let v: i32 = serde::Deserialize::deserialize(d)?;
                    Ok(v)
                }
            }
            struct Foo {
                #[serde(with = "my_format")]
                value: i32,
            }
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        // "serialize" and "deserialize" are universal methods, so they'd be excluded anyway
        // but let's make sure neither triggers
        assert!(
            !warnings.iter().any(|w| w.function_name == "serialize"),
            "Function referenced via #[serde(with)] should not be flagged"
        );
    }

    #[test]
    fn test_serde_default_without_value_ignored() {
        // #[serde(default)] without = "fn" should not crash or extract anything
        let code = r#"
            fn unused_fn() { let x = 1; }
            #[derive(serde::Deserialize)]
            struct Foo {
                #[serde(default)]
                value: i32,
            }
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            warnings.iter().any(|w| w.function_name == "unused_fn"),
            "Unrelated unused function should still be flagged"
        );
    }

    #[test]
    fn test_serde_default_fn_cross_file_not_dead_code() {
        // File A defines default functions, File B uses them via #[serde(default = "...")]
        let code_a = r#"
            pub fn default_true() -> bool { true }
            pub fn default_adx_period() -> u32 { 14 }
        "#;
        let code_b = r#"
            #[derive(serde::Deserialize)]
            struct Config {
                #[serde(default = "default_true")]
                enabled: bool,
                #[serde(default = "default_adx_period")]
                adx_period: u32,
            }
        "#;
        let ast_a = syn::parse_file(code_a).expect("parse code_a");
        let ast_b = syn::parse_file(code_b).expect("parse code_b");
        let parsed = vec![
            (
                "src/config_defaults.rs".to_string(),
                code_a.to_string(),
                ast_a,
            ),
            ("src/config.rs".to_string(), code_b.to_string(), ast_b),
        ];
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        assert!(
            !warnings.iter().any(|w| w.function_name == "default_true"),
            "default_true referenced via #[serde(default)] in another file should not be flagged"
        );
        assert!(
            !warnings
                .iter()
                .any(|w| w.function_name == "default_adx_period"),
            "default_adx_period referenced via #[serde(default)] in another file should not be flagged"
        );
    }

    #[test]
    fn test_serde_default_fn_realistic_pattern() {
        let code = r#"
            fn default_true() -> bool { true }
            fn default_false() -> bool { false }
            fn default_period() -> u32 { 14 }
            fn default_threshold() -> f64 { 0.5 }

            #[derive(serde::Deserialize)]
            struct IndicatorConfig {
                #[serde(default = "default_true")]
                enabled: bool,
                #[serde(default = "default_false")]
                verbose: bool,
                #[serde(default = "default_period")]
                period: u32,
                #[serde(default = "default_threshold")]
                threshold: f64,
            }
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let warnings = detect_dead_code(&parsed, &config, &std::collections::HashMap::new());
        let flagged: Vec<&str> = warnings.iter().map(|w| w.function_name.as_str()).collect();
        assert!(
            !flagged.contains(&"default_true"),
            "default_true should not be flagged, got: {flagged:?}"
        );
        assert!(
            !flagged.contains(&"default_false"),
            "default_false should not be flagged, got: {flagged:?}"
        );
        assert!(
            !flagged.contains(&"default_period"),
            "default_period should not be flagged, got: {flagged:?}"
        );
        assert!(
            !flagged.contains(&"default_threshold"),
            "default_threshold should not be flagged, got: {flagged:?}"
        );
    }

    #[test]
    fn test_call_inside_assert_detected_as_test_call() {
        let code = r#"
            fn helper() -> bool { true }
            #[cfg(test)]
            mod tests {
                use super::*;
                #[test]
                fn test_it() {
                    assert!(helper());
                }
            }
        "#;
        let parsed = parse(code);
        let cfg_test_files = collect_cfg_test_file_paths(&parsed);
        let (_prod_calls, test_calls) = collect_all_calls(&parsed, &cfg_test_files);
        assert!(
            test_calls.contains("helper"),
            "Call inside assert!() should be in test_calls"
        );
    }

    #[test]
    fn test_call_inside_assert_eq_detected() {
        let code = r#"
            fn compute() -> usize { 42 }
            #[cfg(test)]
            mod tests {
                use super::*;
                #[test]
                fn test_it() {
                    assert_eq!(compute(), 42);
                }
            }
        "#;
        let parsed = parse(code);
        let cfg_test_files = collect_cfg_test_file_paths(&parsed);
        let (_prod, test_calls) = collect_all_calls(&parsed, &cfg_test_files);
        assert!(
            test_calls.contains("compute"),
            "Call inside assert_eq!() should be in test_calls"
        );
    }

    #[test]
    fn test_collect_cfg_test_file_paths_inline_mod_ignored() {
        // Inline #[cfg(test)] mod (with body) should NOT produce entries
        let code = r#"
            #[cfg(test)]
            mod tests {
                fn helper() {}
            }
        "#;
        let ast = syn::parse_file(code).unwrap();
        let parsed = vec![("src/lib.rs".to_string(), code.to_string(), ast)];
        let result = collect_cfg_test_file_paths(&parsed);
        assert!(
            result.is_empty(),
            "Inline cfg(test) mod should not produce cfg-test file entries"
        );
    }

    // ── API marker tests ─────────────────────────────────────────

    #[test]
    fn test_api_function_excluded_from_dead_code() {
        let code = r#"
            // qual:api
            pub fn public_api() { let x = 1; }
            fn internal_unused() { let y = 2; }
        "#;
        let parsed = parse(code);
        let config = Config::default();
        let mut api_lines = std::collections::HashMap::new();
        api_lines.insert(
            "test.rs".to_string(),
            [2usize]
                .into_iter()
                .collect::<std::collections::HashSet<_>>(),
        );
        let warnings = detect_dead_code(&parsed, &config, &api_lines);
        let names: Vec<&str> = warnings.iter().map(|w| w.function_name.as_str()).collect();
        assert!(
            !names.contains(&"public_api"),
            "API-marked function should be excluded"
        );
        assert!(
            names.contains(&"internal_unused"),
            "Non-API function should still be flagged"
        );
    }

    #[test]
    fn test_api_does_not_count_as_suppression() {
        // Verify parse_suppression returns None for qual:api
        assert!(crate::findings::parse_suppression(1, "// qual:api").is_none());
    }
}
