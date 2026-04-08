pub mod cohesion;
pub mod module;
mod union_find;

use std::collections::HashSet;

use syn::visit::Visit;

use crate::config::sections::SrpConfig;

/// Warning about a struct that may violate the Single Responsibility Principle.
#[derive(Debug, Clone)]
pub struct SrpWarning {
    pub struct_name: String,
    pub file: String,
    pub line: usize,
    pub lcom4: usize,
    pub field_count: usize,
    pub method_count: usize,
    pub fan_out: usize,
    pub composite_score: f64,
    pub clusters: Vec<ResponsibilityCluster>,
    pub suppressed: bool,
}

/// A cluster of methods that share field accesses (connected component in LCOM4).
#[derive(Debug, Clone)]
pub struct ResponsibilityCluster {
    pub methods: Vec<String>,
    pub fields: Vec<String>,
}

/// Warning about a module with too many production lines or too many independent clusters.
#[derive(Debug, Clone)]
pub struct ModuleSrpWarning {
    pub module: String,
    pub file: String,
    pub production_lines: usize,
    pub length_score: f64,
    /// Number of independent function clusters (0 = not computed or fully connected).
    pub independent_clusters: usize,
    /// Names of functions in each independent cluster.
    pub cluster_names: Vec<Vec<String>>,
    pub suppressed: bool,
}

/// Warning about a function with too many parameters (SRP-004).
#[derive(Debug, Clone)]
pub struct ParamSrpWarning {
    pub function_name: String,
    pub file: String,
    pub line: usize,
    pub parameter_count: usize,
    pub suppressed: bool,
}

/// Complete SRP analysis results.
pub struct SrpAnalysis {
    pub struct_warnings: Vec<SrpWarning>,
    pub module_warnings: Vec<ModuleSrpWarning>,
    pub param_warnings: Vec<ParamSrpWarning>,
}

/// Information about a struct collected from the AST.
pub(crate) struct StructInfo {
    pub name: String,
    pub file: String,
    pub line: usize,
    pub fields: Vec<String>,
}

/// Field access and call data for a single method.
pub(crate) struct MethodFieldData {
    pub method_name: String,
    pub parent_type: String,
    pub field_accesses: HashSet<String>,
    pub call_targets: HashSet<String>,
    /// Method names called on self (e.g. `self.conn()`).
    pub self_method_calls: HashSet<String>,
    /// True if this is a constructor (static method returning Self).
    pub is_constructor: bool,
}

/// Run SRP analysis on all parsed files.
/// Integration: orchestrates struct collection, method data collection,
/// struct-level analysis, and module-level analysis.
pub fn analyze_srp(
    parsed: &[(String, String, syn::File)],
    config: &SrpConfig,
    file_call_graph: &std::collections::HashMap<String, Vec<(String, Vec<String>)>>,
) -> SrpAnalysis {
    let mut structs = Vec::new();
    let mut struct_collector = StructCollector {
        file: String::new(),
        structs: &mut structs,
    };
    crate::dry::visit_all_files(parsed, &mut struct_collector);

    let mut methods = Vec::new();
    let mut method_collector = ImplMethodCollector {
        file: String::new(),
        methods: &mut methods,
    };
    crate::dry::visit_all_files(parsed, &mut method_collector);

    let struct_warnings = cohesion::build_struct_warnings(&structs, &methods, config);
    let module_warnings = module::analyze_module_srp(parsed, config, file_call_graph);
    let param_warnings = Vec::new();
    SrpAnalysis {
        struct_warnings,
        module_warnings,
        param_warnings,
    }
}

/// AST visitor that collects struct definitions with their named fields.
struct StructCollector<'a> {
    file: String,
    structs: &'a mut Vec<StructInfo>,
}

impl crate::dry::FileVisitor for StructCollector<'_> {
    fn reset_for_file(&mut self, file_path: &str) {
        self.file = file_path.to_string();
    }
}

impl<'ast, 'a> Visit<'ast> for StructCollector<'a> {
    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        let fields: Vec<String> = node
            .fields
            .iter()
            .filter_map(|f| f.ident.as_ref().map(|id| id.to_string()))
            .collect();
        // Only track named-field structs (skip tuple structs and unit structs)
        if !fields.is_empty() {
            self.structs.push(StructInfo {
                name: node.ident.to_string(),
                file: self.file.clone(),
                line: node.ident.span().start().line,
                fields,
            });
        }
        syn::visit::visit_item_struct(self, node);
    }
}

/// AST visitor that collects method field accesses and call targets from impl blocks.
struct ImplMethodCollector<'a> {
    file: String,
    methods: &'a mut Vec<MethodFieldData>,
}

impl crate::dry::FileVisitor for ImplMethodCollector<'_> {
    fn reset_for_file(&mut self, file_path: &str) {
        self.file = file_path.to_string();
    }
}

impl<'ast, 'a> Visit<'ast> for ImplMethodCollector<'a> {
    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let type_name = if let syn::Type::Path(tp) = &*node.self_ty {
            tp.path.segments.last().map(|s| s.ident.to_string())
        } else {
            None
        };
        let Some(type_name) = type_name else {
            syn::visit::visit_item_impl(self, node);
            return;
        };
        // Skip trait impls for SRP analysis (Display, Default, etc. are not "own" methods)
        if node.trait_.is_some() {
            syn::visit::visit_item_impl(self, node);
            return;
        }

        for item in &node.items {
            if let syn::ImplItem::Fn(method) = item {
                let is_instance = method.sig.receiver().is_some();
                let is_constructor = !is_instance && returns_self(&method.sig.output);
                if !is_instance && !is_constructor {
                    continue;
                }
                let mut body_visitor = MethodBodyVisitor {
                    field_accesses: HashSet::new(),
                    call_targets: HashSet::new(),
                    self_method_calls: HashSet::new(),
                };
                body_visitor.visit_block(&method.block);
                self.methods.push(MethodFieldData {
                    method_name: method.sig.ident.to_string(),
                    parent_type: type_name.clone(),
                    field_accesses: body_visitor.field_accesses,
                    call_targets: body_visitor.call_targets,
                    self_method_calls: body_visitor.self_method_calls,
                    is_constructor,
                });
            }
        }
        // Don't call default visit — we already handled methods manually
    }
}

/// Visitor that walks a method body to find self.field accesses and call targets.
struct MethodBodyVisitor {
    field_accesses: HashSet<String>,
    call_targets: HashSet<String>,
    self_method_calls: HashSet<String>,
}

impl<'ast> Visit<'ast> for MethodBodyVisitor {
    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        match expr {
            // Detect self.field_name
            syn::Expr::Field(ef) => {
                if is_self_expr(&ef.base) {
                    if let syn::Member::Named(ident) = &ef.member {
                        self.field_accesses.insert(ident.to_string());
                    }
                }
                syn::visit::visit_expr(self, expr);
            }
            // Detect function calls for fan-out: Type::method() or function()
            syn::Expr::Call(ec) => {
                if let syn::Expr::Path(ep) = &*ec.func {
                    let path_str = ep
                        .path
                        .segments
                        .iter()
                        .map(|s| s.ident.to_string())
                        .collect::<Vec<_>>()
                        .join("::");
                    self.call_targets.insert(path_str);
                }
                syn::visit::visit_expr(self, expr);
            }
            // Detect method calls: obj.method()
            syn::Expr::MethodCall(mc) => {
                if is_self_expr(&mc.receiver) {
                    self.self_method_calls.insert(mc.method.to_string());
                } else {
                    self.call_targets.insert(mc.method.to_string());
                }
                syn::visit::visit_expr(self, expr);
            }
            _ => {
                syn::visit::visit_expr(self, expr);
            }
        }
    }
}

/// Check if a function's return type contains Self (constructor pattern).
/// Handles `-> Self`, `-> Result<Self, E>`, `-> Option<Self>`, etc.
/// Operation: pattern matching with closures for IOSP.
fn returns_self(output: &syn::ReturnType) -> bool {
    let syn::ReturnType::Type(_, ty) = output else {
        return false;
    };
    let syn::Type::Path(tp) = &**ty else {
        return false;
    };
    // Direct Self
    if tp.path.segments.last().is_some_and(|s| s.ident == "Self") {
        return true;
    }
    // Self inside one level of generics: Result<Self, E>, Option<Self>, etc.
    tp.path.segments.iter().any(|seg| {
        matches!(&seg.arguments, syn::PathArguments::AngleBracketed(args)
            if args.args.iter().any(|arg| matches!(arg,
                syn::GenericArgument::Type(syn::Type::Path(inner))
                if inner.path.segments.last().is_some_and(|s| s.ident == "Self")
            ))
        )
    })
}

/// Check if an expression is `self`.
/// Operation: pattern matching.
fn is_self_expr(expr: &syn::Expr) -> bool {
    if let syn::Expr::Path(ep) = expr {
        ep.path.is_ident("self")
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_returns_self_yes() {
        let ret: syn::ReturnType = syn::parse_quote!(-> Self);
        assert!(returns_self(&ret));
    }

    #[test]
    fn test_returns_self_result_self() {
        let ret: syn::ReturnType = syn::parse_quote!(-> Result<Self, String>);
        assert!(returns_self(&ret));
    }

    #[test]
    fn test_returns_self_option_self() {
        let ret: syn::ReturnType = syn::parse_quote!(-> Option<Self>);
        assert!(returns_self(&ret));
    }

    #[test]
    fn test_returns_self_no() {
        let ret: syn::ReturnType = syn::parse_quote!(-> i32);
        assert!(!returns_self(&ret));
    }

    #[test]
    fn test_returns_self_result_non_self() {
        let ret: syn::ReturnType = syn::parse_quote!(-> Result<String, Error>);
        assert!(!returns_self(&ret));
    }

    #[test]
    fn test_returns_self_default_return() {
        let ret = syn::ReturnType::Default;
        assert!(!returns_self(&ret));
    }

    #[test]
    fn test_is_self_expr_true() {
        let expr: syn::Expr = syn::parse_quote!(self);
        assert!(is_self_expr(&expr));
    }

    #[test]
    fn test_is_self_expr_false_other_path() {
        let expr: syn::Expr = syn::parse_quote!(other);
        assert!(!is_self_expr(&expr));
    }

    #[test]
    fn test_is_self_expr_false_literal() {
        let expr: syn::Expr = syn::parse_quote!(42);
        assert!(!is_self_expr(&expr));
    }

    fn parse_file(code: &str) -> syn::File {
        syn::parse_file(code).expect("Failed to parse test code")
    }

    /// Test helper: collect structs via visit_all_files (same as analyze_srp uses).
    fn collect_structs(parsed: &[(String, String, syn::File)]) -> Vec<StructInfo> {
        let mut result = Vec::new();
        let mut collector = StructCollector {
            file: String::new(),
            structs: &mut result,
        };
        crate::dry::visit_all_files(parsed, &mut collector);
        result
    }

    /// Test helper: collect methods via visit_all_files (same as analyze_srp uses).
    fn collect_methods(parsed: &[(String, String, syn::File)]) -> Vec<MethodFieldData> {
        let mut result = Vec::new();
        let mut collector = ImplMethodCollector {
            file: String::new(),
            methods: &mut result,
        };
        crate::dry::visit_all_files(parsed, &mut collector);
        result
    }

    #[test]
    fn test_struct_collector_named_fields() {
        let code = "struct Foo { x: i32, y: String }";
        let syntax = parse_file(code);
        let parsed = vec![("test.rs".to_string(), code.to_string(), syntax)];
        let result = collect_structs(&parsed);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "Foo");
        assert_eq!(result[0].fields, vec!["x", "y"]);
    }

    #[test]
    fn test_struct_collector_tuple_struct_skipped() {
        let code = "struct Point(i32, i32);";
        let syntax = parse_file(code);
        let parsed = vec![("test.rs".to_string(), code.to_string(), syntax)];
        let result = collect_structs(&parsed);
        assert!(result.is_empty(), "Tuple structs should be skipped");
    }

    #[test]
    fn test_struct_collector_unit_struct_skipped() {
        let code = "struct Marker;";
        let syntax = parse_file(code);
        let parsed = vec![("test.rs".to_string(), code.to_string(), syntax)];
        let result = collect_structs(&parsed);
        assert!(result.is_empty(), "Unit structs should be skipped");
    }

    #[test]
    fn test_impl_method_collector_instance_methods_and_constructors() {
        let code = r#"
            struct Foo { x: i32, y: i32 }
            impl Foo {
                fn new(x: i32, y: i32) -> Self { Self { x, y } }
                fn get_x(&self) -> i32 { self.x }
                fn set_y(&mut self, y: i32) { self.y = y; }
                fn helper() -> i32 { 42 }
            }
        "#;
        let syntax = parse_file(code);
        let parsed = vec![("test.rs".to_string(), code.to_string(), syntax)];
        let result = collect_methods(&parsed);
        // Instance methods (get_x, set_y) + constructor (new) — helper is static (no self, no -> Self)
        assert_eq!(result.len(), 3);
        let names: Vec<&str> = result.iter().map(|m| m.method_name.as_str()).collect();
        assert!(names.contains(&"get_x"));
        assert!(names.contains(&"set_y"));
        assert!(names.contains(&"new"));
        // Verify constructor flag
        let new_method = result.iter().find(|m| m.method_name == "new").unwrap();
        assert!(new_method.is_constructor);
        let get_x_method = result.iter().find(|m| m.method_name == "get_x").unwrap();
        assert!(!get_x_method.is_constructor);
    }

    #[test]
    fn test_impl_method_collector_skips_trait_impls() {
        let code = r#"
            struct Foo { x: i32 }
            impl Foo {
                fn get_x(&self) -> i32 { self.x }
            }
            impl std::fmt::Display for Foo {
                fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    write!(f, "{}", self.x)
                }
            }
        "#;
        let syntax = parse_file(code);
        let parsed = vec![("test.rs".to_string(), code.to_string(), syntax)];
        let result = collect_methods(&parsed);
        // Only inherent methods, not trait impls
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].method_name, "get_x");
    }

    #[test]
    fn test_method_body_visitor_field_accesses() {
        let code = r#"
            struct Foo { x: i32, y: i32, z: i32 }
            impl Foo {
                fn sum(&self) -> i32 { self.x + self.y }
            }
        "#;
        let syntax = parse_file(code);
        let parsed = vec![("test.rs".to_string(), code.to_string(), syntax)];
        let result = collect_methods(&parsed);
        assert_eq!(result.len(), 1);
        assert!(result[0].field_accesses.contains("x"));
        assert!(result[0].field_accesses.contains("y"));
        assert!(!result[0].field_accesses.contains("z"));
    }

    #[test]
    fn test_method_body_visitor_call_targets() {
        let code = r#"
            struct Foo { data: Vec<i32> }
            impl Foo {
                fn process(&self) -> usize { helper(self.data.len()) }
            }
            fn helper(n: usize) -> usize { n }
        "#;
        let syntax = parse_file(code);
        let parsed = vec![("test.rs".to_string(), code.to_string(), syntax)];
        let result = collect_methods(&parsed);
        assert_eq!(result.len(), 1);
        assert!(result[0].call_targets.contains("helper"));
    }

    #[test]
    fn test_analyze_srp_empty() {
        let parsed: Vec<(String, String, syn::File)> = vec![];
        let config = SrpConfig::default();
        let call_graph = std::collections::HashMap::new();
        let analysis = analyze_srp(&parsed, &config, &call_graph);
        assert!(analysis.struct_warnings.is_empty());
        assert!(analysis.module_warnings.is_empty());
    }

    #[test]
    fn test_analyze_srp_cohesive_struct() {
        let code = r#"
            struct Counter { count: usize }
            impl Counter {
                fn increment(&mut self) { self.count += 1; }
                fn get(&self) -> usize { self.count }
                fn reset(&mut self) { self.count = 0; }
            }
        "#;
        let syntax = parse_file(code);
        let parsed = vec![("test.rs".to_string(), code.to_string(), syntax)];
        let config = SrpConfig::default();
        let call_graph = std::collections::HashMap::new();
        let analysis = analyze_srp(&parsed, &config, &call_graph);
        // Fully cohesive struct → no warning
        assert!(
            analysis.struct_warnings.is_empty(),
            "Cohesive struct should not trigger SRP warning"
        );
    }

    #[test]
    fn test_analyze_srp_multiple_files() {
        let code1 = "struct A { x: i32 }\nimpl A { fn get_x(&self) -> i32 { self.x } }";
        let code2 = "struct B { y: i32 }\nimpl B { fn get_y(&self) -> i32 { self.y } }";
        let syntax1 = parse_file(code1);
        let syntax2 = parse_file(code2);
        let parsed = vec![
            ("a.rs".to_string(), code1.to_string(), syntax1),
            ("b.rs".to_string(), code2.to_string(), syntax2),
        ];
        let config = SrpConfig::default();
        let call_graph = std::collections::HashMap::new();
        let analysis = analyze_srp(&parsed, &config, &call_graph);
        // Both structs are simple → no warnings
        assert!(analysis.struct_warnings.is_empty());
    }

    #[test]
    fn test_analyze_srp_returns_empty_param_warnings() {
        // param_warnings are now populated by the pipeline, not analyze_srp
        let parsed: Vec<(String, String, syn::File)> = vec![];
        let config = SrpConfig::default();
        let call_graph = std::collections::HashMap::new();
        let analysis = analyze_srp(&parsed, &config, &call_graph);
        assert!(analysis.param_warnings.is_empty());
    }
}
