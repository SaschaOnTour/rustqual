use std::collections::HashSet;

use syn::visit::Visit;

use crate::dry::DeclaredFunction;
use crate::scope::ProjectScope;

use super::{TqWarning, TqWarningKind};

/// Detect test functions that do not call any production function (TQ-002).
/// Operation: iterates test functions, compares call targets against known prod functions.
pub(crate) fn detect_no_sut_tests(
    parsed: &[(String, String, syn::File)],
    scope: &ProjectScope,
    declared_fns: &[DeclaredFunction],
    reaches_prod: &HashSet<String>,
) -> Vec<TqWarning> {
    // Build set of known production function names
    let prod_fn_names: HashSet<&str> = declared_fns
        .iter()
        .filter(|f| !f.is_test)
        .map(|f| f.name.as_str())
        .collect();

    let mut warnings = Vec::new();
    for (path, _, syntax) in parsed {
        let mut collector = TestCallCollector::default();
        collector.visit_file(syntax);
        for test_fn in &collector.test_fns {
            let calls_prod = test_fn.call_targets.iter().any(|target| {
                prod_fn_names.contains(target.as_str())
                    || scope.functions.contains(target)
                    || scope.methods.contains(target)
                    || reaches_prod.contains(target)
            }) || test_fn.type_qualified_calls.iter().any(|type_name| {
                scope.types.contains(type_name)
            });
            if !calls_prod {
                warnings.push(TqWarning {
                    file: path.clone(),
                    line: test_fn.line,
                    function_name: test_fn.name.clone(),
                    kind: TqWarningKind::NoSut,
                    suppressed: false,
                });
            }
        }
    }
    warnings
}

/// A collected test function with its call targets.
struct TestFnWithCalls {
    name: String,
    line: usize,
    call_targets: Vec<String>,
    /// Type names from `Type::method()` calls (for recognizing constructor/static method calls).
    type_qualified_calls: Vec<String>,
}

/// Collects `#[test]` functions and the functions they call.
#[derive(Default)]
struct TestCallCollector {
    test_fns: Vec<TestFnWithCalls>,
    in_test_fn: bool,
    current_calls: Vec<String>,
    current_type_calls: Vec<String>,
}

impl<'ast> Visit<'ast> for TestCallCollector {
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if self.in_test_fn {
            // Parse macro arguments (assert!, assert_eq!, vec!, etc.) as expressions
            // to find function calls embedded inside macros.
            use syn::punctuated::Punctuated;
            if let Ok(args) = syn::parse::Parser::parse2(
                Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated,
                node.tokens.clone(),
            ) {
                args.iter()
                    .for_each(|expr| syn::visit::visit_expr(self, expr));
            }
        }
        syn::visit::visit_macro(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if has_test_attr(&node.attrs) {
            self.in_test_fn = true;
            self.current_calls.clear();
            self.current_type_calls.clear();
            syn::visit::visit_item_fn(self, node);
            self.in_test_fn = false;
            let line = node.sig.ident.span().start().line;
            self.test_fns.push(TestFnWithCalls {
                name: node.sig.ident.to_string(),
                line,
                call_targets: std::mem::take(&mut self.current_calls),
                type_qualified_calls: std::mem::take(&mut self.current_type_calls),
            });
        } else {
            syn::visit::visit_item_fn(self, node);
        }
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if self.in_test_fn {
            if let syn::Expr::Path(ref p) = *node.func {
                let name = path_to_name(&p.path);
                self.current_calls.push(name);
                if let Some(type_name) = path_type_prefix(&p.path) {
                    self.current_type_calls.push(type_name);
                }
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if self.in_test_fn {
            self.current_calls.push(node.method.to_string());
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_struct(&mut self, node: &'ast syn::ExprStruct) {
        // Recognize production struct construction as exercising SUT
        if self.in_test_fn {
            if let Some(last) = node.path.segments.last() {
                self.current_type_calls.push(last.ident.to_string());
            }
        }
        syn::visit::visit_expr_struct(self, node);
    }

    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        // Recognize enum variant paths (e.g. Dimension::Iosp → type "Dimension")
        if self.in_test_fn && node.path.segments.len() >= 2 {
            let type_seg = &node.path.segments[node.path.segments.len() - 2];
            self.current_type_calls.push(type_seg.ident.to_string());
        }
        syn::visit::visit_expr_path(self, node);
    }
}

/// Extract the final segment name from a path.
/// Operation: path segment extraction.
fn path_to_name(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default()
}

/// Extract the type prefix from a 2+-segment path (e.g. "Config" from "Config::load").
/// Operation: path prefix extraction.
fn path_type_prefix(path: &syn::Path) -> Option<String> {
    let len = path.segments.len();
    if len >= 2 {
        path.segments.iter().nth(len - 2).map(|s| s.ident.to_string())
    } else {
        None
    }
}

use crate::dry::has_test_attr;

#[cfg(test)]
mod tests {
    use super::*;

    fn make_declared(name: &str, is_test: bool) -> DeclaredFunction {
        DeclaredFunction {
            name: name.to_string(),
            qualified_name: name.to_string(),
            file: "lib.rs".to_string(),
            line: 1,
            is_test,
            is_main: false,
            is_trait_impl: false,
            has_allow_dead_code: false,
            is_api: false,
        }
    }

    fn parse_and_detect(source: &str, declared: &[DeclaredFunction]) -> Vec<TqWarning> {
        let syntax = syn::parse_file(source).expect("test source");
        let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
        let scope_source = "fn prod_fn() {} fn helper() {}";
        let scope_syntax = syn::parse_file(scope_source).expect("scope source");
        let scope_refs = vec![("lib.rs", &scope_syntax)];
        let scope = ProjectScope::from_files(&scope_refs);
        let reaches_prod = HashSet::new();
        detect_no_sut_tests(&parsed, &scope, declared, &reaches_prod)
    }

    #[test]
    fn test_calls_prod_function_no_warning() {
        let declared = vec![make_declared("prod_fn", false)];
        let warnings = parse_and_detect(
            r#"
            #[test]
            fn test_it() {
                prod_fn();
                assert!(true);
            }
            "#,
            &declared,
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_calls_only_external_emits_warning() {
        let declared = vec![make_declared("prod_fn", false)];
        let warnings = parse_and_detect(
            r#"
            #[test]
            fn test_it() {
                let x = 42;
            }
            "#,
            &declared,
        );
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].kind, TqWarningKind::NoSut);
    }

    #[test]
    fn test_calls_method_on_scope_no_warning() {
        let declared = vec![make_declared("helper", false)];
        let warnings = parse_and_detect(
            r#"
            #[test]
            fn test_it() {
                helper();
            }
            "#,
            &declared,
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_non_test_function_ignored() {
        let declared = vec![make_declared("prod_fn", false)];
        let warnings = parse_and_detect(
            r#"
            fn not_a_test() {
                let x = 42;
            }
            "#,
            &declared,
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_empty_test_emits_warning() {
        let declared = vec![make_declared("prod_fn", false)];
        let warnings = parse_and_detect(
            r#"
            #[test]
            fn test_empty() {}
            "#,
            &declared,
        );
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].function_name, "test_empty");
    }

    #[test]
    fn test_type_constructor_recognized_as_sut() {
        let declared = vec![make_declared("new", false)];
        // Build a scope that includes a type named "MyType"
        let scope_source = "struct MyType {} impl MyType { fn new() -> Self { MyType {} } }";
        let scope_syntax = syn::parse_file(scope_source).expect("scope source");
        let scope_refs = vec![("lib.rs", &scope_syntax)];
        let scope = ProjectScope::from_files(&scope_refs);
        // Test: calling MyType::new() should count as SUT
        let source = r#"
            #[test]
            fn test_constructor() {
                let x = MyType::new();
            }
        "#;
        let syntax = syn::parse_file(source).expect("test source");
        let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
        let reaches_prod = HashSet::new();
        let warnings = detect_no_sut_tests(&parsed, &scope, &declared, &reaches_prod);
        assert!(warnings.is_empty(), "MyType::new() should be recognized as SUT call");
    }

    #[test]
    fn test_static_method_recognized_as_sut() {
        let declared = vec![make_declared("load", false)];
        let scope_source = "struct Config {} impl Config { fn load() -> Self { Config {} } }";
        let scope_syntax = syn::parse_file(scope_source).expect("scope source");
        let scope_refs = vec![("lib.rs", &scope_syntax)];
        let scope = ProjectScope::from_files(&scope_refs);
        let source = r#"
            #[test]
            fn test_load() {
                let c = Config::load();
            }
        "#;
        let syntax = syn::parse_file(source).expect("test source");
        let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
        let reaches_prod = HashSet::new();
        let warnings = detect_no_sut_tests(&parsed, &scope, &declared, &reaches_prod);
        assert!(warnings.is_empty(), "Config::load() should be recognized as SUT call");
    }

    #[test]
    fn test_transitive_sut_via_helper() {
        let declared = vec![make_declared("prod_fn", false)];
        let scope_source = "fn prod_fn() {}";
        let scope_syntax = syn::parse_file(scope_source).expect("scope source");
        let scope_refs = vec![("lib.rs", &scope_syntax)];
        let scope = ProjectScope::from_files(&scope_refs);
        let source = r#"
            #[test]
            fn test_via_helper() {
                my_helper();
            }
        "#;
        let syntax = syn::parse_file(source).expect("test source");
        let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
        // my_helper transitively reaches prod_fn
        let reaches_prod: HashSet<String> = ["my_helper".to_string()].into();
        let warnings = detect_no_sut_tests(&parsed, &scope, &declared, &reaches_prod);
        assert!(
            warnings.is_empty(),
            "my_helper transitively calls prod code"
        );
    }
}
