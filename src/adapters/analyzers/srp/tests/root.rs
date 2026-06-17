use crate::adapters::analyzers::srp::*;

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
        module_stack: Vec::new(),
        structs: &mut result,
    };
    crate::adapters::shared::file_visitor::visit_all_files(parsed, &mut collector);
    result
}

/// Test helper: collect methods via visit_all_files (same as analyze_srp uses).
fn collect_methods(parsed: &[(String, String, syn::File)]) -> Vec<MethodFieldData> {
    let mut result = Vec::new();
    let mut bridges = Vec::new();
    let mut collector = ImplMethodCollector {
        file: String::new(),
        module_stack: Vec::new(),
        methods: &mut result,
        bridges: &mut bridges,
    };
    crate::adapters::shared::file_visitor::visit_all_files(parsed, &mut collector);
    result
}

/// Run the full `analyze_srp` path over a single in-memory source and return
/// just its struct warnings — the shared arrange for the source-level
/// struct-SRP tests, so each test body is only its own source + oracle.
fn struct_warnings_for(path: &str, code: &str) -> Vec<SrpWarning> {
    let parsed = vec![(path.to_string(), code.to_string(), parse_file(code))];
    analyze_srp(
        &parsed,
        &SrpConfig::default(),
        &std::collections::HashMap::new(),
        300,
    )
    .struct_warnings
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
    let analysis = analyze_srp(&parsed, &config, &call_graph, 300);
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
    // Fully cohesive struct → no warning
    assert!(
        struct_warnings_for("test.rs", code).is_empty(),
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
    let analysis = analyze_srp(&parsed, &config, &call_graph, 300);
    // Both structs are simple → no warnings
    assert!(analysis.struct_warnings.is_empty());
}

#[test]
fn test_analyze_srp_returns_empty_param_warnings() {
    // param_warnings are now populated by the pipeline, not analyze_srp
    let parsed: Vec<(String, String, syn::File)> = vec![];
    let config = SrpConfig::default();
    let call_graph = std::collections::HashMap::new();
    let analysis = analyze_srp(&parsed, &config, &call_graph, 300);
    assert!(analysis.param_warnings.is_empty());
}

#[test]
fn test_analyze_srp_god_struct_fires_in_test_file() {
    // SRP-001 (god-struct cohesion) deliberately applies to TEST code too: a
    // god-fixture wiring up auth + db + caching + routing in one struct is a
    // real smell, and the "independent #[test] fns are the file's purpose"
    // rationale that makes the MODULE cluster-check production-only does NOT
    // excuse it. Only `analyze_module_srp` skips test files; the struct-level
    // check is intentionally NOT gated on cfg_test_files (production thresholds,
    // no separate `[tests]` knob — `// qual:allow(srp)` covers rare fixtures).
    // The `#![cfg(test)]` inner attr marks this a whole test file, so this pins
    // the struct warning still fires there.
    let code = r#"
        #![cfg(test)]
        struct GodFixture {
            db: u8, cache: u8, logger: u8, metrics: u8, config: u8, state: u8,
            buffer: u8, queue: u8, pool: u8, handler: u8, router: u8, auth: u8,
        }
        impl GodFixture {
            fn read_db(&self) { query(self.db); }
            fn write_db(&mut self) { commit(self.db); }
            fn read_cache(&self) { get_key(self.cache); }
            fn write_cache(&mut self) { set_key(self.cache); }
            fn log_info(&self) { format_log(self.logger); }
            fn log_error(&self) { format_log(self.logger); inc(self.metrics); }
            fn route(&self) { dispatch(self.router, self.handler); }
            fn authenticate(&self) { verify(self.auth, self.config); }
            fn flush(&mut self) { drain(self.buffer, self.queue); }
            fn manage_pool(&mut self) { alloc(self.pool, self.state); }
        }
    "#;
    assert!(
        struct_warnings_for("tests/fixtures.rs", code)
            .iter()
            .any(|w| w.struct_name == "GodFixture"),
        "struct SRP-001 must fire on a god-struct even in a test file"
    );
}
