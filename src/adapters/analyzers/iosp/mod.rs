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
}

impl<'a> Analyzer<'a> {
    pub fn new(config: &'a Config, scope: &'a ProjectScope) -> Self {
        Self { config, scope }
    }

    /// Analyze all functions in a parsed syn::File.
    /// Trivial: iterator chain with logic in closures (lenient).
    pub fn analyze_file(&self, file: &File, file_path: &str) -> Vec<FunctionAnalysis> {
        file.items
            .iter()
            .flat_map(|item| match item {
                Item::Fn(f) => self
                    .analyze_item_fn(f, file_path, None, false)
                    .into_iter()
                    .collect::<Vec<_>>(),
                Item::Impl(i) => {
                    let test = crate::adapters::analyzers::dry::has_cfg_test(&i.attrs);
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
            fa.is_test = in_test || crate::adapters::analyzers::dry::has_test_attr(&item_fn.attrs);
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
        let mod_is_test = in_test || crate::adapters::analyzers::dry::has_cfg_test(&item_mod.attrs);
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
                                || crate::adapters::analyzers::dry::has_cfg_test(&i.attrs);
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
mod tests {
    use super::*;
    use crate::adapters::analyzers::iosp::scope::ProjectScope;
    use crate::config::Config;

    /// Helper: parse code, build scope from it, analyze with default config.
    fn parse_and_analyze(code: &str) -> Vec<FunctionAnalysis> {
        let syntax = syn::parse_file(code).expect("Failed to parse test code");
        let scope_files = vec![("test.rs", &syntax)];
        let scope = ProjectScope::from_files(&scope_files);
        let config = Config::default();
        let analyzer = Analyzer::new(&config, &scope);
        let mut results = analyzer.analyze_file(&syntax, "test.rs");
        let parsed = vec![("test.rs".to_string(), code.to_string(), syntax)];
        let recursive_lines = crate::adapters::source::filesystem::collect_recursive_lines(&parsed);
        crate::pipeline::warnings::apply_recursive_annotations(&mut results, &recursive_lines);
        crate::pipeline::warnings::apply_leaf_reclassification(&mut results);
        results
    }

    /// Helper: parse code with a custom config.
    fn parse_and_analyze_with_config(code: &str, config: &Config) -> Vec<FunctionAnalysis> {
        let syntax = syn::parse_file(code).expect("Failed to parse test code");
        let scope_files = vec![("test.rs", &syntax)];
        let scope = ProjectScope::from_files(&scope_files);
        let analyzer = Analyzer::new(config, &scope);
        analyzer.analyze_file(&syntax, "test.rs")
    }

    // ---------------------------------------------------------------
    // Classification Tests
    // ---------------------------------------------------------------

    #[test]
    fn test_pure_integration() {
        let code = r#"
            fn helper_a() {}
            fn helper_b() {}
            fn integrator() {
                helper_a();
                helper_b();
            }
        "#;
        let results = parse_and_analyze(code);
        let integrator = results.iter().find(|r| r.name == "integrator").unwrap();
        assert_eq!(integrator.classification, Classification::Integration);
    }

    #[test]
    fn test_pure_operation() {
        let code = r#"
            fn operation(x: i32) -> &'static str {
                let _y = x;
                if _y > 0 {
                    "positive"
                } else {
                    "non-positive"
                }
            }
        "#;
        let results = parse_and_analyze(code);
        let op = results.iter().find(|r| r.name == "operation").unwrap();
        assert_eq!(op.classification, Classification::Operation);
    }

    #[test]
    fn test_violation_mixed() {
        let code = r#"
            fn helper(x: i32) { if x > 0 { violator(x); } }
            fn violator(x: i32) {
                let _y = x;
                if _y > 0 {
                    helper(_y);
                }
            }
        "#;
        let results = parse_and_analyze(code);
        let v = results.iter().find(|r| r.name == "violator").unwrap();
        assert!(
            matches!(v.classification, Classification::Violation { .. }),
            "Expected Violation, got {:?}",
            v.classification
        );
    }

    #[test]
    fn test_violation_locations() {
        let code = r#"fn helper(x: i32) { if x > 0 { violator(x); } }
fn violator(x: i32) {
    let _y = x;
    if _y > 0 {
        helper(_y);
    }
}
"#;
        let results = parse_and_analyze(code);
        let v = results.iter().find(|r| r.name == "violator").unwrap();
        if let Classification::Violation {
            logic_locations,
            call_locations,
            ..
        } = &v.classification
        {
            assert!(
                logic_locations
                    .iter()
                    .any(|l| l.kind == "if" && l.line == 4),
                "Expected 'if' on line 4, got: {:?}",
                logic_locations
            );
            assert!(
                call_locations
                    .iter()
                    .any(|c| c.name == "helper" && c.line == 5),
                "Expected 'helper' call on line 5, got: {:?}",
                call_locations
            );
        } else {
            panic!("Expected Violation, got {:?}", v.classification);
        }
    }

    #[test]
    fn test_trivial_empty_body() {
        let code = r#"
            fn f() {}
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        assert_eq!(f.classification, Classification::Trivial);
    }

    #[test]
    fn test_trivial_single_return() {
        let code = r#"
            fn f() -> i32 { 42 }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        assert_eq!(f.classification, Classification::Trivial);
    }

    #[test]
    fn test_trivial_getter() {
        let code = r#"
            struct Foo { x: i32 }
            impl Foo {
                fn get_x(&self) -> i32 { self.x }
            }
        "#;
        let results = parse_and_analyze(code);
        let getter = results.iter().find(|r| r.name == "get_x").unwrap();
        assert_eq!(getter.classification, Classification::Trivial);
    }

    #[test]
    fn test_single_stmt_with_own_call() {
        let code = r#"
            fn helper() {}
            fn f() { helper() }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        assert_eq!(
            f.classification,
            Classification::Integration,
            "Single-statement body with own call should be Integration, got {:?}",
            f.classification
        );
    }

    #[test]
    fn test_single_stmt_with_logic() {
        let code = r#"
            fn f(x: i32) -> i32 { if x > 0 { 1 } else { 0 } }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        assert_eq!(
            f.classification,
            Classification::Operation,
            "Single-statement body with logic should be Operation, got {:?}",
            f.classification
        );
    }

    // ---------------------------------------------------------------
    // Closure Tests
    // ---------------------------------------------------------------

    #[test]
    fn test_closure_lenient_ignores_logic() {
        let code = r#"
            fn f() {
                let v = vec![1, 2, 3];
                let _: Vec<_> = v.into_iter().collect();
                let _ = (|| { if true { 1 } else { 2 } })();
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        assert!(
            !matches!(f.classification, Classification::Violation { .. }),
            "Logic inside a closure should not cause a violation in lenient mode, got {:?}",
            f.classification
        );
    }

    #[test]
    fn test_closure_strict_counts_logic() {
        let mut config = Config::default();
        config.strict_closures = true;
        let code = r#"
            fn f() {
                let v = vec![1, 2, 3];
                let _ = (|| { if true { 1 } else { 2 } })();
                let _ = v.len();
            }
        "#;
        let results = parse_and_analyze_with_config(code, &config);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        assert!(
            matches!(
                f.classification,
                Classification::Operation | Classification::Violation { .. }
            ),
            "Expected logic to be counted in strict closure mode, got {:?}",
            f.classification
        );
    }

    #[test]
    fn test_closure_lenient_ignores_calls() {
        let code = r#"
            fn helper() {}
            fn f() {
                let c = || { helper(); };
                c();
                let _ = 1;
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        assert!(
            !matches!(f.classification, Classification::Violation { .. }),
            "Own call inside closure should be ignored in lenient mode, got {:?}",
            f.classification
        );
    }

    #[test]
    fn test_closure_strict_counts_calls() {
        let mut config = Config::default();
        config.strict_closures = true;
        let code = r#"
            fn helper() {}
            fn f() {
                let c = || { helper(); };
                c();
                let _ = 1;
            }
        "#;
        let results = parse_and_analyze_with_config(code, &config);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        assert!(
            matches!(
                f.classification,
                Classification::Integration | Classification::Violation { .. }
            ),
            "Own call inside closure should be counted in strict mode, got {:?}",
            f.classification
        );
    }

    // ---------------------------------------------------------------
    // Iterator Tests
    // ---------------------------------------------------------------

    #[test]
    fn test_iterator_lenient_not_own_call() {
        let code = r#"
            fn f() -> Vec<i32> {
                let v = vec![1, 2, 3];
                v.iter().map(|x| x + 1).collect()
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        assert!(
            !matches!(f.classification, Classification::Violation { .. }),
            "Iterator methods should not be own calls in lenient mode, got {:?}",
            f.classification
        );
    }

    #[test]
    fn test_iterator_strict_counts_as_logic() {
        let mut config = Config::default();
        config.strict_iterator_chains = true;
        let code = r#"
            struct Foo;
            impl Foo {
                fn map(&self) {}
            }
            fn f() -> Vec<i32> {
                let v = vec![1, 2, 3];
                let x = v.iter().map(|x| x + 1).collect::<Vec<_>>();
                x
            }
        "#;
        let results = parse_and_analyze_with_config(code, &config);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        assert!(
            matches!(
                f.classification,
                Classification::Integration | Classification::Violation { .. }
            ),
            "Iterator methods should be counted in strict mode when in scope, got {:?}",
            f.classification
        );
    }

    // ---------------------------------------------------------------
    // ProjectScope Tests
    // ---------------------------------------------------------------

    #[test]
    fn test_method_call_own_type() {
        let code = r#"
            struct MyStruct;
            impl MyStruct {
                fn do_work(&self) {}
                fn orchestrate(&self) {
                    self.do_work();
                    self.do_work();
                }
            }
        "#;
        let results = parse_and_analyze(code);
        let orch = results.iter().find(|r| r.name == "orchestrate").unwrap();
        assert_eq!(orch.classification, Classification::Integration);
    }

    #[test]
    fn test_method_call_external() {
        let code = r#"
            fn operation_fn() {
                let mut v = Vec::new();
                if v.is_empty() {
                    v.push(1);
                }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "operation_fn").unwrap();
        assert_eq!(f.classification, Classification::Operation);
    }

    #[test]
    fn test_function_call_own() {
        let code = r#"
            fn step_a() {}
            fn step_b() {}
            fn orchestrate() {
                step_a();
                step_b();
            }
        "#;
        let results = parse_and_analyze(code);
        let orch = results.iter().find(|r| r.name == "orchestrate").unwrap();
        assert_eq!(orch.classification, Classification::Integration);
    }

    #[test]
    fn test_path_call_own_type() {
        let code = r#"
            struct MyType;
            impl MyType {
                fn create() -> Self { MyType }
            }
            fn f() {
                let _a = MyType::create();
                let _b = MyType::create();
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        assert_eq!(f.classification, Classification::Integration);
    }

    #[test]
    fn test_path_call_external_type() {
        let code = r#"
            fn f() {
                let _a = String::new();
                let _b = String::new();
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        assert!(
            !matches!(f.classification, Classification::Integration),
            "String::new() should not be counted as own call, got {:?}",
            f.classification
        );
    }

    // ---------------------------------------------------------------
    // Structure Tests
    // ---------------------------------------------------------------

    #[test]
    fn test_impl_block_parent_type() {
        let code = r#"
            struct Foo;
            impl Foo {
                fn bar(&self) {}
            }
        "#;
        let results = parse_and_analyze(code);
        let bar = results.iter().find(|r| r.name == "bar").unwrap();
        assert_eq!(bar.parent_type, Some("Foo".to_string()));
    }

    #[test]
    fn test_trait_default_impl() {
        let code = r#"
            fn step() {}
            trait MyTrait {
                fn default_method(&self) {
                    step();
                    step();
                }
            }
        "#;
        let results = parse_and_analyze(code);
        let dm = results.iter().find(|r| r.name == "default_method").unwrap();
        assert_eq!(dm.classification, Classification::Integration);
        assert_eq!(dm.parent_type, Some("MyTrait".to_string()));
    }

    #[test]
    fn test_ignored_function_skipped() {
        let mut config = Config::default();
        config.ignore_functions.push("test_*".to_string());
        let code = r#"
            fn test_something() {
                if true { }
            }
            fn real_function() -> i32 { 42 }
        "#;
        let results = parse_and_analyze_with_config(code, &config);
        assert!(
            results.iter().all(|r| r.name != "test_something"),
            "Ignored function should not appear in results"
        );
        assert!(
            results.iter().any(|r| r.name == "real_function"),
            "Non-ignored function should appear in results"
        );
    }

    #[test]
    fn test_nested_module() {
        let code = r#"
            mod inner {
                fn nested_fn() -> i32 { 42 }
            }
        "#;
        let results = parse_and_analyze(code);
        let nested = results.iter().find(|r| r.name == "nested_fn").unwrap();
        assert_eq!(nested.classification, Classification::Trivial);
    }

    // ---------------------------------------------------------------
    // A2: Recursion Tests
    // ---------------------------------------------------------------

    #[test]
    fn test_recursion_default_is_violation() {
        let code = r#"
            fn fib(n: u32) -> u32 {
                let _x = n;
                if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
            }
        "#;
        let results = parse_and_analyze(code);
        let fib = results.iter().find(|r| r.name == "fib").unwrap();
        assert!(
            matches!(fib.classification, Classification::Violation { .. }),
            "Recursive function should be Violation by default, got {:?}",
            fib.classification
        );
    }

    #[test]
    fn test_recursion_allowed_becomes_operation() {
        let mut config = Config::default();
        config.allow_recursion = true;
        let code = r#"
            fn fib(n: u32) -> u32 {
                let _x = n;
                if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
            }
        "#;
        let results = parse_and_analyze_with_config(code, &config);
        let fib = results.iter().find(|r| r.name == "fib").unwrap();
        assert_eq!(
            fib.classification,
            Classification::Operation,
            "Recursive function with allow_recursion should be Operation, got {:?}",
            fib.classification
        );
    }

    // ---------------------------------------------------------------
    // A3: Error Propagation Tests
    // ---------------------------------------------------------------

    #[test]
    fn test_question_mark_default_not_logic() {
        let code = r#"
            fn f() -> Result<(), String> {
                let _x = 1;
                let _y: Result<i32, String> = Ok(1);
                Ok(())
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        assert!(
            !matches!(f.classification, Classification::Violation { .. }),
            "? operator should not count as logic by default"
        );
    }

    #[test]
    fn test_question_mark_strict_counts_as_logic() {
        let mut config = Config::default();
        config.strict_error_propagation = true;
        let code = r#"
            fn helper() -> Result<i32, String> { Ok(42) }
            fn f() -> Result<(), String> {
                let _x = helper()?;
                let _ = 1;
                Ok(())
            }
        "#;
        let results = parse_and_analyze_with_config(code, &config);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        assert!(
            matches!(f.classification, Classification::Violation { .. }),
            "? operator should count as logic with strict_error_propagation, got {:?}",
            f.classification
        );
    }

    // ---------------------------------------------------------------
    // A4: Async/Await Tests
    // ---------------------------------------------------------------

    #[test]
    fn test_async_block_lenient_ignores_logic() {
        let code = r#"
            fn f() {
                let _ = async { if true { 1 } else { 2 } };
                let _ = 1;
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        assert!(
            !matches!(f.classification, Classification::Violation { .. }),
            "Logic inside async block should be ignored in lenient mode, got {:?}",
            f.classification
        );
    }

    // ---------------------------------------------------------------
    // G1: Complexity Metrics Tests
    // ---------------------------------------------------------------

    #[test]
    fn test_complexity_metrics_present() {
        let code = r#"
            fn f(x: i32) {
                let _y = x;
                if x > 0 {
                    if x > 10 {
                        let _ = x + 1;
                    }
                }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        let metrics = f
            .complexity
            .as_ref()
            .expect("Should have complexity metrics");
        assert!(metrics.logic_count > 0, "Should have logic count");
        assert!(metrics.max_nesting > 0, "Should have nesting depth");
    }

    #[test]
    fn test_complexity_nesting_depth() {
        let code = r#"
            fn f(x: i32) {
                let _y = x;
                if x > 0 {
                    if x > 10 {
                        while x > 100 {
                            break;
                        }
                    }
                }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        let metrics = f.complexity.as_ref().unwrap();
        assert_eq!(
            metrics.max_nesting, 3,
            "Expected nesting depth 3 (if > if > while)"
        );
    }

    // ---------------------------------------------------------------
    // C2: Severity Tests
    // ---------------------------------------------------------------

    #[test]
    fn test_severity_low() {
        let code = r#"
            fn helper(x: bool) { if x { f(false); } }
            fn f(x: bool) {
                let _y = x;
                if x { helper(true); }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        assert_eq!(f.severity, Some(Severity::Low));
    }

    #[test]
    fn test_severity_none_for_non_violation() {
        let code = r#"
            fn f(x: i32) {
                let _y = x;
                if x > 0 { }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        assert_eq!(f.severity, None);
    }

    // ---------------------------------------------------------------
    // Suppression Tests
    // ---------------------------------------------------------------

    #[test]
    fn test_suppressed_flag_default_false() {
        let code = r#"
            fn f() {}
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        assert!(!f.suppressed);
    }

    // ---------------------------------------------------------------
    // D1/D7: qualified_name + severity fields
    // ---------------------------------------------------------------

    #[test]
    fn test_qualified_name_free_fn() {
        let code = r#"
            fn my_function() {}
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "my_function").unwrap();
        assert_eq!(f.qualified_name, "my_function");
    }

    #[test]
    fn test_qualified_name_impl_method() {
        let code = r#"
            struct Foo;
            impl Foo {
                fn bar(&self) {}
            }
        "#;
        let results = parse_and_analyze(code);
        let bar = results.iter().find(|r| r.name == "bar").unwrap();
        assert_eq!(bar.qualified_name, "Foo::bar");
    }

    // ---------------------------------------------------------------
    // Bug Fix: Trivial Self-Getter Not Violation
    // ---------------------------------------------------------------

    #[test]
    fn test_trivial_self_getter_not_violation() {
        let code = r#"
            struct Counter { count: usize }
            impl Counter {
                fn symbol_count(&self) -> usize { self.count }
                fn next_symbol(&self) -> usize {
                    if self.symbol_count() > 0 {
                        self.symbol_count() + 1
                    } else {
                        0
                    }
                }
            }
        "#;
        let results = parse_and_analyze(code);
        let next = results.iter().find(|r| r.name == "next_symbol").unwrap();
        assert_eq!(
            next.classification,
            Classification::Operation,
            "Trivial getter should not make next_symbol a Violation, got {:?}",
            next.classification
        );
    }

    // ---------------------------------------------------------------
    // Bug Fix: Type::new() Not Own Call
    // ---------------------------------------------------------------

    #[test]
    fn test_leaf_constructor_call_is_operation() {
        // Adx::new() is Trivial (leaf). Calling a leaf + logic = Operation.
        let code = r#"
            struct Adx { period: usize }
            impl Adx {
                fn new(period: usize) -> Self { Adx { period } }
            }
            fn compute(data: &[f64]) -> f64 {
                let indicator = Adx::new(14);
                if data.is_empty() { 0.0 } else { data[0] }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "compute").unwrap();
        assert!(
            matches!(f.classification, Classification::Operation),
            "Adx::new() is leaf → calling it + logic = Operation, got {:?}",
            f.classification
        );
    }

    // ---------------------------------------------------------------
    // Bug Fix: Trivial .get() Getter Not Violation
    // ---------------------------------------------------------------

    #[test]
    fn test_trivial_getter_get_not_violation() {
        let code = r#"
            struct Browser { results: Vec<String>, selected: usize }
            impl Browser {
                fn current(&self) -> Option<&String> { self.results.get(self.selected) }
                fn process(&self) -> String {
                    if let Some(item) = self.current() {
                        item.clone()
                    } else {
                        String::new()
                    }
                }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "process").unwrap();
        assert_eq!(
            f.classification,
            Classification::Operation,
            "Trivial .get() getter should not make process a Violation, got {:?}",
            f.classification
        );
    }

    // ---------------------------------------------------------------
    // Bug Fix: For-Loop Delegation Not Violation
    // ---------------------------------------------------------------

    #[test]
    fn test_for_loop_delegation_not_violation() {
        let code = r#"
            fn process(_x: i32) {}
            fn f(items: Vec<i32>) {
                for x in items {
                    process(x);
                }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "f").unwrap();
        assert_eq!(
            f.classification,
            Classification::Integration,
            "For-loop delegation should be Integration, got {:?}",
            f.classification
        );
    }

    // ---------------------------------------------------------------
    // Bug Fix: Match-Dispatch Delegation Not Violation
    // ---------------------------------------------------------------

    #[test]
    fn test_match_dispatch_is_integration() {
        let code = r#"
            fn call_a() {}
            fn call_b() {}
            fn dispatch(x: i32) {
                match x {
                    0 => call_a(),
                    _ => call_b(),
                }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "dispatch").unwrap();
        assert_eq!(
            f.classification,
            Classification::Integration,
            "Match dispatch should be Integration, got {:?}",
            f.classification
        );
    }

    #[test]
    fn test_match_dispatch_method_is_integration() {
        let code = r#"
            struct S;
            impl S {
                fn run_a(&self) {}
                fn run_b(&self) {}
                fn dispatch(&self, x: i32) {
                    match x {
                        0 => self.run_a(),
                        _ => self.run_b(),
                    }
                }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "dispatch").unwrap();
        assert_eq!(
            f.classification,
            Classification::Integration,
            "Match method dispatch should be Integration, got {:?}",
            f.classification
        );
    }

    #[test]
    fn test_match_with_logic_in_arm_is_violation() {
        let code = r#"
            fn call_a(_x: i32) { if _x > 0 { dispatch(_x - 1); } }
            fn call_b() { dispatch(0); }
            fn dispatch(x: i32) {
                match x {
                    0 => call_a(x + 1),
                    _ => call_b(),
                }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "dispatch").unwrap();
        assert!(
            matches!(f.classification, Classification::Violation { .. }),
            "Match with logic in arm should be Violation, got {:?}",
            f.classification
        );
    }

    #[test]
    fn test_match_with_guard_is_violation() {
        let code = r#"
            fn call_a() { if true { dispatch(0); } }
            fn call_b() { dispatch(1); }
            fn dispatch(x: i32) {
                match x {
                    n if n > 0 => call_a(),
                    _ => call_b(),
                }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "dispatch").unwrap();
        assert!(
            matches!(f.classification, Classification::Violation { .. }),
            "Match with guard should be Violation, got {:?}",
            f.classification
        );
    }

    #[test]
    fn test_match_dispatch_complexity_still_tracked() {
        let code = r#"
            fn call_a() {}
            fn call_b() {}
            fn dispatch(x: i32) {
                match x {
                    0 => call_a(),
                    _ => call_b(),
                }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|r| r.name == "dispatch").unwrap();
        assert_eq!(
            f.classification,
            Classification::Integration,
            "Match dispatch should be Integration, got {:?}",
            f.classification
        );
        assert!(
            f.complexity.as_ref().unwrap().cognitive_complexity >= 1,
            "Complexity should still be tracked for dispatch match"
        );
    }

    // ---------------------------------------------------------------
    // is_test detection tests
    // ---------------------------------------------------------------

    #[test]
    fn test_fn_with_test_attr_is_test() {
        let code = r#"
            fn helper() {}
            #[test]
            fn my_test() {
                helper();
                if true {}
            }
        "#;
        let results = parse_and_analyze(code);
        let test_fn = results.iter().find(|f| f.name == "my_test").unwrap();
        assert!(
            test_fn.is_test,
            "Function with #[test] should have is_test=true"
        );
    }

    #[test]
    fn test_fn_inside_cfg_test_mod_is_test() {
        let code = r#"
            fn production_fn() {}
            #[cfg(test)]
            mod tests {
                fn test_helper() {}
            }
        "#;
        let results = parse_and_analyze(code);
        let prod = results.iter().find(|f| f.name == "production_fn").unwrap();
        assert!(
            !prod.is_test,
            "Production function should have is_test=false"
        );
        let helper = results.iter().find(|f| f.name == "test_helper").unwrap();
        assert!(
            helper.is_test,
            "Function inside #[cfg(test)] mod should have is_test=true"
        );
    }

    #[test]
    fn test_regular_fn_not_test() {
        let code = r#"
            fn regular() { if true {} }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|f| f.name == "regular").unwrap();
        assert!(!f.is_test, "Regular function should have is_test=false");
    }

    #[test]
    fn test_cfg_test_impl_methods_are_test() {
        let code = r#"
            pub struct Config { pub name: String }

            impl Config {
                pub fn new(name: String) -> Self { Self { name } }
            }

            #[cfg(test)]
            impl Config {
                fn test_helper(&self) -> bool { true }
                pub fn another_helper() -> i32 { if true { 1 } else { 2 } }
            }
        "#;
        let results = parse_and_analyze(code);
        let helper = results.iter().find(|f| f.name == "test_helper").unwrap();
        assert!(
            helper.is_test,
            "Method inside #[cfg(test)] impl should have is_test=true"
        );
        let another = results.iter().find(|f| f.name == "another_helper").unwrap();
        assert!(
            another.is_test,
            "Pub method inside #[cfg(test)] impl should have is_test=true"
        );
        // Regular impl method should NOT be test
        let new_fn = results.iter().find(|f| f.name == "new").unwrap();
        assert!(
            !new_fn.is_test,
            "Method in regular impl should have is_test=false"
        );
    }

    // ---------------------------------------------------------------
    // Bug 2: Method-call type resolution tests
    // ---------------------------------------------------------------

    #[test]
    fn test_method_on_non_project_type_not_own_call() {
        // Cache defines .clear(), but reset_name calls .clear() on a String parameter.
        // String::clear is NOT an own call — different type.
        let code = r#"
            struct Cache { data: Vec<i32> }
            impl Cache {
                fn clear(&mut self) {
                    self.data = Vec::new();
                }
            }
            fn reset_name(name: &mut String) {
                if name.is_empty() { return; }
                name.clear();
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|f| f.name == "reset_name").unwrap();
        assert!(
            matches!(f.classification, Classification::Operation),
            "name.clear() is String::clear, not Cache::clear — should be Operation, got {:?}",
            f.classification
        );
    }

    #[test]
    fn test_self_method_call_is_own_call() {
        // self.process() IS an own call — it's on the same type
        let code = r#"
            struct Engine;
            impl Engine {
                fn process(&self) -> i32 { 42 }
                fn run(&self) {
                    self.process();
                }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|f| f.name == "run").unwrap();
        assert!(
            matches!(f.classification, Classification::Integration),
            "self.process() is own call — should be Integration, got {:?}",
            f.classification
        );
    }

    #[test]
    fn test_method_on_param_project_type_is_own_call() {
        // db.query() where db is a project type parameter — IS an own call
        let code = r#"
            struct Database;
            impl Database {
                fn query(&self) -> Vec<String> { vec![] }
            }
            fn fetch(db: &Database) {
                db.query();
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|f| f.name == "fetch").unwrap();
        assert!(
            matches!(f.classification, Classification::Integration),
            "db.query() on project type param — should be Integration, got {:?}",
            f.classification
        );
    }

    #[test]
    fn test_method_name_collision_resolved_by_type() {
        // Both Formatter and Vec have "push". Formatter::push is own,
        // but v.push() on a Vec parameter should NOT be an own call.
        let code = r#"
            struct Formatter { parts: Vec<String> }
            impl Formatter {
                fn push(&mut self, s: String) {
                    self.parts.push(s);
                }
            }
            fn collect_items(v: &mut Vec<String>) {
                if v.is_empty() { return; }
                v.push("done".to_string());
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|f| f.name == "collect_items").unwrap();
        assert!(
            matches!(f.classification, Classification::Operation),
            "v.push() on Vec param — not an own call, should be Operation, got {:?}",
            f.classification
        );
    }

    // ---------------------------------------------------------------
    // Automatic leaf detection tests
    // ---------------------------------------------------------------

    #[test]
    fn test_leaf_call_not_counted_as_own_call() {
        // get_config is a leaf (C=0, Operation).
        // cmd_quality calls get_config + has logic → should be Operation (leaf calls don't count).
        let code = r#"
            fn get_config() -> i32 {
                if true { 1 } else { 2 }
            }
            fn cmd_quality(clear: bool) -> i32 {
                let config = get_config();
                if clear { config + 1 } else { config }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|f| f.name == "cmd_quality").unwrap();
        assert!(
            matches!(f.classification, Classification::Operation),
            "Calling a leaf (get_config) + logic should be Operation, got {:?}",
            f.classification
        );
    }

    #[test]
    fn test_non_leaf_call_still_violation() {
        // bad_a and bad_b form a cycle — both are Violations that can't be reclassified.
        // caller has logic + calls bad_a → stays Violation.
        let code = r#"
            fn bad_a(x: bool) -> i32 {
                if x { bad_b(false) } else { 0 }
            }
            fn bad_b(x: bool) -> i32 {
                if x { bad_a(true) } else { 1 }
            }
            fn caller(x: bool) -> i32 {
                if x { bad_a(true) } else { 0 }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|f| f.name == "caller").unwrap();
        assert!(
            matches!(f.classification, Classification::Violation { .. }),
            "Calling a non-leaf (orchestrator) + logic should be Violation, got {:?}",
            f.classification
        );
    }

    #[test]
    fn test_multiple_leaf_calls_still_operation() {
        // Both helpers are leaves (C=0). Calling multiple leaves + logic → Operation.
        let code = r#"
            fn validate(s: &str) -> bool { s.len() > 3 }
            fn normalize(s: &str) -> String { s.to_lowercase() }
            fn process(input: &str) -> Option<String> {
                if validate(input) {
                    Some(normalize(input))
                } else {
                    None
                }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|f| f.name == "process").unwrap();
        assert!(
            matches!(f.classification, Classification::Operation),
            "Calling only leaves + logic should be Operation, got {:?}",
            f.classification
        );
    }

    #[test]
    fn test_pure_integration_unchanged() {
        // Integration (only calls, no logic) stays Integration — unaffected by leaf detection.
        let code = r#"
            fn step_a() {}
            fn step_b() {}
            fn pipeline() {
                step_a();
                step_b();
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|f| f.name == "pipeline").unwrap();
        assert!(
            matches!(f.classification, Classification::Integration),
            "Pure Integration should stay Integration, got {:?}",
            f.classification
        );
    }

    #[test]
    fn test_cascading_leaf_detection() {
        // step_a and step_b are leaves (C=0).
        // middle calls only leaves → after leaf detection, middle is Operation → also a leaf.
        // top calls middle + has logic → should be Operation (middle is transitively a leaf).
        let code = r#"
            fn step_a() -> i32 { if true { 1 } else { 0 } }
            fn step_b() -> i32 { 42 }
            fn middle() -> i32 {
                if step_a() > 0 { step_b() } else { 0 }
            }
            fn top(x: bool) -> i32 {
                if x { middle() } else { -1 }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|f| f.name == "top").unwrap();
        assert!(
            matches!(f.classification, Classification::Operation),
            "Cascading leaf: top calls middle (which calls only leaves) should be Operation, got {:?}",
            f.classification
        );
    }

    // ---------------------------------------------------------------
    // qual:recursive annotation tests
    // ---------------------------------------------------------------

    #[test]
    fn test_recursive_annotation_makes_self_call_safe() {
        let code = r#"
            // qual:recursive
            fn traverse(node: &str) -> i32 {
                if node.is_empty() { return 0; }
                traverse(node)
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|f| f.name == "traverse").unwrap();
        assert!(
            matches!(f.classification, Classification::Operation),
            "qual:recursive should make self-call safe → Operation, got {:?}",
            f.classification
        );
    }

    #[test]
    fn test_recursive_without_annotation_is_violation() {
        let code = r#"
            fn inner() {}
            fn traverse(node: &str) -> i32 {
                if node.is_empty() { return 0; }
                inner();
                traverse(node)
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|f| f.name == "traverse").unwrap();
        assert!(
            matches!(f.classification, Classification::Violation { .. }),
            "Without annotation, recursive + non-leaf call + logic = Violation, got {:?}",
            f.classification
        );
    }

    // ---------------------------------------------------------------
    // Integration-as-safe-target tests
    // ---------------------------------------------------------------

    #[test]
    fn test_call_to_integration_is_safe() {
        let code = r#"
            fn log_action() {}
            fn db_save() { log_action(); }
            fn handler(x: bool) -> i32 {
                if x { db_save(); 1 } else { 0 }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|f| f.name == "handler").unwrap();
        assert!(
            matches!(f.classification, Classification::Operation),
            "Call to Integration (L=0, C>0) + logic should be Operation, got {:?}",
            f.classification
        );
    }

    #[test]
    fn test_call_to_violation_stays_violation() {
        let code = r#"
            fn bad_a(x: bool) -> i32 {
                if x { bad_b(false) } else { 0 }
            }
            fn bad_b(x: bool) -> i32 {
                if x { bad_a(true) } else { 1 }
            }
            fn caller(y: bool) -> i32 {
                if y { bad_a(true) } else { -1 }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|f| f.name == "caller").unwrap();
        assert!(
            matches!(f.classification, Classification::Violation { .. }),
            "Call to mutually-recursive Violation + logic should stay Violation, got {:?}",
            f.classification
        );
    }

    #[test]
    fn test_mixed_leaf_and_integration_calls_safe() {
        let code = r#"
            fn log_it() {}
            fn get_config() -> i32 { if true { 1 } else { 2 } }
            fn db_fetch() -> i32 { log_it(); 42 }
            fn process(x: bool) -> i32 {
                let cfg = get_config();
                if x { db_fetch() + cfg } else { cfg }
            }
        "#;
        let results = parse_and_analyze(code);
        let f = results.iter().find(|f| f.name == "process").unwrap();
        assert!(
            matches!(f.classification, Classification::Operation),
            "Calls to leaf + integration + logic should be Operation, got {:?}",
            f.classification
        );
    }
}
