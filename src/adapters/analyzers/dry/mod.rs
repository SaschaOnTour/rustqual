pub mod boilerplate;
pub(crate) mod call_targets;
pub(crate) mod cfg_test_detection;
pub mod dead_code;
pub mod fragments;
pub mod functions;
pub mod match_patterns;
pub mod wildcards;

pub use boilerplate::BoilerplateFind;
pub use dead_code::{DeadCodeKind, DeadCodeWarning};
pub use fragments::FragmentGroup;
pub use functions::{DuplicateGroup, DuplicateKind};

use syn::visit::Visit;

use crate::adapters::shared::normalize::NormalizedToken;

// ── Shared visitor infrastructure ──────────────────────────────

/// Trait for AST visitors that need per-file state reset.
pub(crate) trait FileVisitor {
    fn reset_for_file(&mut self, file_path: &str);
}

/// Visit all parsed files with a visitor, resetting per-file state.
/// Trivial: iteration with trait method call.
pub(crate) fn visit_all_files<'a, V>(parsed: &'a [(String, String, syn::File)], visitor: &mut V)
where
    V: FileVisitor + Visit<'a>,
{
    parsed.iter().for_each(|(path, _, file)| {
        visitor.reset_for_file(path);
        syn::visit::visit_file(visitor, file);
    });
}

// ── Shared types ────────────────────────────────────────────────

/// A function with its normalized hash information, ready for duplicate detection.
pub struct FunctionHashEntry {
    pub name: String,
    pub qualified_name: String,
    pub file: String,
    pub line: usize,
    pub hash: u64,
    pub token_count: usize,
    pub tokens: Vec<NormalizedToken>,
}

/// A declared function with metadata for dead code analysis.
pub struct DeclaredFunction {
    pub name: String,
    pub qualified_name: String,
    pub file: String,
    pub line: usize,
    pub is_test: bool,
    pub is_main: bool,
    pub is_trait_impl: bool,
    pub has_allow_dead_code: bool,
    /// Whether this function is marked as public API via `// qual:api`.
    pub is_api: bool,
}

// ── Function hash collection ────────────────────────────────────

/// Collect function hashes from all parsed files.
/// Trivial: creates visitor and delegates to visit_all_files.
pub(crate) fn collect_function_hashes(
    parsed: &[(String, String, syn::File)],
    config: &crate::config::sections::DuplicatesConfig,
) -> Vec<FunctionHashEntry> {
    let mut collector = functions::FunctionCollector::new(config);
    visit_all_files(parsed, &mut collector);
    collector.entries
}

/// Collect declared function metadata from all parsed files.
/// Trivial: creates visitor and delegates to visit_all_files.
pub(crate) fn collect_declared_functions(
    parsed: &[(String, String, syn::File)],
) -> Vec<DeclaredFunction> {
    let mut collector = dead_code::DeclaredFnCollector::new();
    visit_all_files(parsed, &mut collector);
    collector.functions
}

// ── Attribute helpers ───────────────────────────────────────────

/// Check if attributes contain `#[cfg(test)]`.
/// Operation: attribute inspection logic.
pub(crate) fn has_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Ident>()
                .is_ok_and(|ident| ident == "test")
    })
}

/// Check if attributes contain `#[test]`.
/// Operation: attribute inspection logic.
pub(crate) fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("test"))
}

/// Check if attributes contain `#[allow(dead_code)]`.
/// Operation: attribute inspection logic.
fn has_allow_dead_code(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("allow")
            && attr
                .parse_args::<syn::Ident>()
                .is_ok_and(|ident| ident == "dead_code")
    })
}

/// Build qualified name from optional parent type and base name.
/// Operation: simple string formatting, no own calls.
fn qualify_name(parent: &Option<String>, name: &str) -> String {
    parent
        .as_ref()
        .map_or_else(|| name.to_string(), |p| [p.as_str(), "::", name].concat())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::sections::DuplicatesConfig;

    fn parse(code: &str) -> Vec<(String, String, syn::File)> {
        let syntax = syn::parse_file(code).expect("parse failed");
        vec![("test.rs".to_string(), code.to_string(), syntax)]
    }

    #[test]
    fn test_collect_function_hashes_empty() {
        let parsed = parse("");
        let config = DuplicatesConfig::default();
        let entries = collect_function_hashes(&parsed, &config);
        assert!(entries.is_empty());
    }

    #[test]
    fn test_collect_function_hashes_small_function_excluded() {
        // A tiny function should be excluded by min_tokens
        let parsed = parse("fn tiny() { let x = 1; }");
        let config = DuplicatesConfig::default(); // min_tokens = 30
        let entries = collect_function_hashes(&parsed, &config);
        assert!(entries.is_empty(), "Small function should be filtered out");
    }

    #[test]
    fn test_collect_function_hashes_large_function_included() {
        // A larger function with many tokens
        let code = r#"
            fn big_fn() {
                let a = 1;
                let b = 2;
                let c = a + b;
                let d = c * a;
                let e = d - b;
                let f = e + c;
                let g = f * d;
                let h = g - e;
                let i = h + f;
                let j = i * g;
            }
        "#;
        let parsed = parse(code);
        let config = DuplicatesConfig {
            min_tokens: 5, // Lower threshold for test
            min_lines: 1,
            ..DuplicatesConfig::default()
        };
        let entries = collect_function_hashes(&parsed, &config);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "big_fn");
    }

    #[test]
    fn test_collect_function_hashes_test_excluded() {
        let code = r#"
            #[cfg(test)]
            mod tests {
                fn helper() {
                    let a = 1; let b = 2; let c = a + b;
                    let d = c * a; let e = d - b; let f = e + c;
                }
            }
        "#;
        let parsed = parse(code);
        let config = DuplicatesConfig {
            min_tokens: 5,
            min_lines: 1,
            ignore_tests: true,
            ..DuplicatesConfig::default()
        };
        let entries = collect_function_hashes(&parsed, &config);
        assert!(entries.is_empty(), "Test functions should be excluded");
    }

    #[test]
    fn test_collect_function_hashes_test_included_when_not_ignored() {
        let code = r#"
            #[cfg(test)]
            mod tests {
                fn helper() {
                    let a = 1; let b = 2; let c = a + b;
                    let d = c * a; let e = d - b; let f = e + c;
                }
            }
        "#;
        let parsed = parse(code);
        let config = DuplicatesConfig {
            min_tokens: 5,
            min_lines: 1,
            ignore_tests: false,
            ..DuplicatesConfig::default()
        };
        let entries = collect_function_hashes(&parsed, &config);
        assert_eq!(entries.len(), 1, "Test functions should be included");
    }

    #[test]
    fn test_collect_function_hashes_impl_method() {
        let code = r#"
            struct Foo;
            impl Foo {
                fn method(&self) {
                    let a = 1; let b = 2; let c = a + b;
                    let d = c * a; let e = d - b; let f = e + c;
                }
            }
        "#;
        let parsed = parse(code);
        let config = DuplicatesConfig {
            min_tokens: 5,
            min_lines: 1,
            ..DuplicatesConfig::default()
        };
        let entries = collect_function_hashes(&parsed, &config);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].qualified_name, "Foo::method");
    }

    #[test]
    fn test_collect_function_hashes_trait_impl_excluded() {
        let code = r#"
            trait Bar { fn do_thing(&self); }
            struct Foo;
            impl Bar for Foo {
                fn do_thing(&self) {
                    let a = 1; let b = 2; let c = a + b;
                    let d = c * a; let e = d - b; let f = e + c;
                }
            }
        "#;
        let parsed = parse(code);
        let config = DuplicatesConfig {
            min_tokens: 5,
            min_lines: 1,
            ignore_trait_impls: true,
            ..DuplicatesConfig::default()
        };
        let entries = collect_function_hashes(&parsed, &config);
        assert!(entries.is_empty(), "Trait impl methods should be excluded");
    }

    #[test]
    fn test_has_cfg_test_positive() {
        let code = "#[cfg(test)] mod tests {}";
        let file = syn::parse_file(code).unwrap();
        if let syn::Item::Mod(m) = &file.items[0] {
            assert!(has_cfg_test(&m.attrs));
        }
    }

    #[test]
    fn test_has_cfg_test_negative() {
        let code = "#[cfg(feature = \"foo\")] mod feature_mod {}";
        let file = syn::parse_file(code).unwrap();
        if let syn::Item::Mod(m) = &file.items[0] {
            assert!(!has_cfg_test(&m.attrs));
        }
    }

    #[test]
    fn test_has_test_attr() {
        let code = "#[test] fn test_something() {}";
        let file = syn::parse_file(code).unwrap();
        if let syn::Item::Fn(f) = &file.items[0] {
            assert!(has_test_attr(&f.attrs));
        }
    }

    #[test]
    fn test_collect_declared_functions_basic() {
        let code = "fn foo() {} fn bar() {} fn main() {}";
        let parsed = parse(code);
        let declared = collect_declared_functions(&parsed);
        assert_eq!(declared.len(), 3);
        assert!(declared.iter().any(|d| d.name == "main" && d.is_main));
        assert!(declared.iter().any(|d| d.name == "foo" && !d.is_main));
    }

    #[test]
    fn test_collect_declared_functions_test_context() {
        let code = r#"
            fn production() {}
            #[cfg(test)]
            mod tests {
                fn helper() {}
                #[test]
                fn test_something() {}
            }
        "#;
        let parsed = parse(code);
        let declared = collect_declared_functions(&parsed);
        let prod = declared.iter().find(|d| d.name == "production").unwrap();
        assert!(!prod.is_test);
        let helper = declared.iter().find(|d| d.name == "helper").unwrap();
        assert!(helper.is_test);
        let test_fn = declared
            .iter()
            .find(|d| d.name == "test_something")
            .unwrap();
        assert!(test_fn.is_test);
    }

    #[test]
    fn test_collect_declared_functions_trait_impl() {
        let code = r#"
            trait Foo { fn bar(&self); }
            struct S;
            impl Foo for S {
                fn bar(&self) {}
            }
        "#;
        let parsed = parse(code);
        let declared = collect_declared_functions(&parsed);
        let bar = declared.iter().find(|d| d.name == "bar").unwrap();
        assert!(bar.is_trait_impl);
    }

    #[test]
    fn test_collect_declared_functions_allow_dead_code() {
        let code = "#[allow(dead_code)] fn unused() {}";
        let parsed = parse(code);
        let declared = collect_declared_functions(&parsed);
        assert_eq!(declared.len(), 1);
        assert!(declared[0].has_allow_dead_code);
    }

    #[test]
    fn test_cfg_test_impl_methods_are_test() {
        let code = r#"
            pub struct Foo;

            #[cfg(test)]
            impl Foo {
                fn test_helper(&self) -> bool { true }
                pub fn another_helper() -> i32 { 42 }
            }
        "#;
        let parsed = parse(code);
        let declared = collect_declared_functions(&parsed);

        let helper = declared.iter().find(|d| d.name == "test_helper").unwrap();
        assert!(
            helper.is_test,
            "Method inside #[cfg(test)] impl should have is_test=true"
        );

        let another = declared
            .iter()
            .find(|d| d.name == "another_helper")
            .unwrap();
        assert!(
            another.is_test,
            "Pub method inside #[cfg(test)] impl should have is_test=true"
        );
    }
}
