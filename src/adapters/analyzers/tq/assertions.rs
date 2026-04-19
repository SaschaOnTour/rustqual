use syn::visit::Visit;

use super::{TqWarning, TqWarningKind};

/// Core assertion macros from std. Detection uses prefix matching (`assert*`, `debug_assert*`)
/// so any crate-provided `assert_*` macro (e.g. `assert_relative_eq!`) is automatically recognized.
/// This list is retained as documentation of the well-known set.
#[cfg(doc)]
const _ASSERTION_MACROS: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
];

/// Detect test functions that have no assertions (TQ-001).
/// Operation: iterates parsed files, visits test functions, checks for assertions.
/// Recognizes all `assert*` macros by prefix, plus any extra macros from config.
pub(crate) fn detect_assertion_free_tests(
    parsed: &[(String, String, syn::File)],
    extra_assertion_macros: &[String],
) -> Vec<TqWarning> {
    let mut warnings = Vec::new();
    for (path, _, syntax) in parsed {
        let mut collector = TestFunctionCollector::default();
        collector.visit_file(syntax);
        for test_fn in &collector.test_fns {
            let mut visitor = TestAssertionVisitor {
                extra_macros: extra_assertion_macros,
                ..Default::default()
            };
            visitor.visit_block(&test_fn.body);
            if !(visitor.has_assertion
                || (test_fn.should_panic && visitor.has_panic)
                || visitor.has_call)
            {
                warnings.push(TqWarning {
                    file: path.clone(),
                    line: test_fn.line,
                    function_name: test_fn.name.clone(),
                    kind: TqWarningKind::NoAssertion,
                    suppressed: false,
                });
            }
        }
    }
    warnings
}

/// A collected test function with its body for assertion analysis.
struct TestFnInfo {
    name: String,
    line: usize,
    body: syn::Block,
    should_panic: bool,
}

/// Collects `#[test]` functions from a file.
#[derive(Default)]
struct TestFunctionCollector {
    test_fns: Vec<TestFnInfo>,
    in_cfg_test: bool,
}

impl<'ast> Visit<'ast> for TestFunctionCollector {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let was_in_cfg_test = self.in_cfg_test;
        if has_cfg_test(&node.attrs) {
            self.in_cfg_test = true;
        }
        syn::visit::visit_item_mod(self, node);
        self.in_cfg_test = was_in_cfg_test;
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if has_test_attr(&node.attrs) {
            let line = node.sig.ident.span().start().line;
            self.test_fns.push(TestFnInfo {
                name: node.sig.ident.to_string(),
                line,
                body: (*node.block).clone(),
                should_panic: has_should_panic_attr(&node.attrs),
            });
        }
        syn::visit::visit_item_fn(self, node);
    }
}

/// Visits a function body looking for assertion macro calls and function calls.
#[derive(Default)]
struct TestAssertionVisitor<'cfg> {
    has_assertion: bool,
    has_panic: bool,
    /// Whether the test body calls any function (implicit no-panic assertion).
    has_call: bool,
    /// Extra macro names (beyond `assert*` prefix) to treat as assertions.
    extra_macros: &'cfg [String],
}

/// Check if a macro name is an assertion: `assert*` or `debug_assert*` prefix, or in extra list.
/// Operation: string prefix + linear search.
fn is_assertion_macro(name: &str, extra_macros: &[String]) -> bool {
    name.starts_with("assert")
        || name.starts_with("debug_assert")
        || extra_macros.iter().any(|m| m == name)
}

impl<'ast> Visit<'ast> for TestAssertionVisitor<'_> {
    fn visit_expr_macro(&mut self, node: &'ast syn::ExprMacro) {
        let macro_name = macro_ident_name(&node.mac);
        if is_assertion_macro(&macro_name, self.extra_macros) {
            self.has_assertion = true;
        }
        if macro_name == "panic" {
            self.has_panic = true;
        }
        syn::visit::visit_expr_macro(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        let macro_name = macro_ident_name(node);
        if is_assertion_macro(&macro_name, self.extra_macros) {
            self.has_assertion = true;
        }
        if macro_name == "panic" {
            self.has_panic = true;
        }
        syn::visit::visit_macro(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        self.has_call = true;
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.has_call = true;
        syn::visit::visit_expr_method_call(self, node);
    }
}

/// Extract the final segment name from a macro path.
/// Operation: path segment extraction.
fn macro_ident_name(mac: &syn::Macro) -> String {
    mac.path
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default()
}

use crate::adapters::analyzers::dry::{has_cfg_test, has_test_attr};

/// Check if attributes contain `#[should_panic]`.
/// Operation: attribute matching.
fn has_should_panic_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("should_panic"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_detect(source: &str) -> Vec<TqWarning> {
        parse_and_detect_with_extras(source, &[])
    }

    fn parse_and_detect_with_extras(source: &str, extras: &[String]) -> Vec<TqWarning> {
        let syntax = syn::parse_file(source).expect("test source");
        let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
        detect_assertion_free_tests(&parsed, extras)
    }

    #[test]
    fn test_with_assert_no_warning() {
        let warnings = parse_and_detect(
            r#"
            #[cfg(test)]
            mod tests {
                #[test]
                fn test_something() {
                    assert!(true);
                }
            }
            "#,
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_with_assert_eq_no_warning() {
        let warnings = parse_and_detect(
            r#"
            #[cfg(test)]
            mod tests {
                #[test]
                fn test_something() {
                    assert_eq!(1, 1);
                }
            }
            "#,
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_without_assertion_emits_warning() {
        let warnings = parse_and_detect(
            r#"
            #[cfg(test)]
            mod tests {
                #[test]
                fn test_something() {
                    let x = 42;
                }
            }
            "#,
        );
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].kind, TqWarningKind::NoAssertion);
    }

    #[test]
    fn test_should_panic_with_panic_no_warning() {
        let warnings = parse_and_detect(
            r#"
            #[cfg(test)]
            mod tests {
                #[test]
                #[should_panic]
                fn test_something() {
                    panic!("expected");
                }
            }
            "#,
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_should_panic_without_panic_emits_warning() {
        let warnings = parse_and_detect(
            r#"
            #[cfg(test)]
            mod tests {
                #[test]
                #[should_panic]
                fn test_something() {
                    let x = 42;
                }
            }
            "#,
        );
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_empty_test_emits_warning() {
        let warnings = parse_and_detect(
            r#"
            #[cfg(test)]
            mod tests {
                #[test]
                fn test_empty() {}
            }
            "#,
        );
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].function_name, "test_empty");
    }

    #[test]
    fn test_debug_assert_no_warning() {
        let warnings = parse_and_detect(
            r#"
            #[cfg(test)]
            mod tests {
                #[test]
                fn test_something() {
                    debug_assert!(true);
                }
            }
            "#,
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_assert_ne_no_warning() {
        let warnings = parse_and_detect(
            r#"
            #[cfg(test)]
            mod tests {
                #[test]
                fn test_something() {
                    assert_ne!(1, 2);
                }
            }
            "#,
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_non_test_function_ignored() {
        let warnings = parse_and_detect(
            r#"
            fn not_a_test() {
                let x = 42;
            }
            "#,
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_assert_prefixed_custom_macro_no_warning() {
        let warnings = parse_and_detect(
            r#"
            #[cfg(test)]
            mod tests {
                #[test]
                fn test_approx() {
                    assert_relative_eq!(1.0, 1.0001, epsilon = 0.01);
                }
            }
            "#,
        );
        assert!(
            warnings.is_empty(),
            "assert_relative_eq! should be recognized by prefix"
        );
    }

    #[test]
    fn test_extra_assertion_macro_config() {
        let extras = vec!["verify".to_string()];
        let warnings = parse_and_detect_with_extras(
            r#"
            #[cfg(test)]
            mod tests {
                #[test]
                fn test_custom() {
                    verify!(result.is_ok());
                }
            }
            "#,
            &extras,
        );
        assert!(
            warnings.is_empty(),
            "verify! in extra_assertion_macros should be recognized"
        );
    }

    #[test]
    fn test_no_assertion_still_warns() {
        let warnings = parse_and_detect(
            r#"
            #[cfg(test)]
            mod tests {
                #[test]
                fn test_nothing() {
                    let _ = 42;
                }
            }
            "#,
        );
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].kind, TqWarningKind::NoAssertion);
    }

    #[test]
    fn test_multiple_tests_mixed() {
        let warnings = parse_and_detect(
            r#"
            #[cfg(test)]
            mod tests {
                #[test]
                fn test_good() {
                    assert!(true);
                }
                #[test]
                fn test_bad() {
                    let x = 42;
                }
            }
            "#,
        );
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].function_name, "test_bad");
    }
}
