use crate::adapters::analyzers::iosp::scope::*;
use std::collections::{HashMap, HashSet};
use syn::visit::Visit;
use syn::{File, ImplItem, TraitItem};

fn build_scope(code: &str) -> ProjectScope {
    let syntax = syn::parse_file(code).expect("Failed to parse test code");
    let files = vec![("test.rs", &syntax)];
    ProjectScope::from_files(&files)
}

// ── ScopeCollector Tests ──────────────────────────────────────────

#[test]
fn test_collect_free_functions() {
    let scope = build_scope("fn foo() {} fn bar() {}");
    assert!(scope.functions.contains("foo"));
    assert!(scope.functions.contains("bar"));
}

#[test]
fn test_collect_impl_methods() {
    let scope = build_scope("struct Foo; impl Foo { fn bar(&self) {} }");
    assert!(scope.methods.contains("bar"));
    assert!(scope.types.contains("Foo"));
}

#[test]
fn test_collect_trait_methods() {
    let scope = build_scope("trait T { fn baz(&self); }");
    assert!(scope.methods.contains("baz"));
    assert!(scope.types.contains("T"));
}

#[test]
fn test_collect_struct_names() {
    let scope = build_scope("struct S;");
    assert!(scope.types.contains("S"));
}

#[test]
fn test_collect_enum_names() {
    let scope = build_scope("enum E { A }");
    assert!(scope.types.contains("E"));
}

#[test]
fn test_collect_nested_module() {
    let scope = build_scope("mod inner { fn f() {} }");
    assert!(scope.functions.contains("f"));
}

// ── is_own_function Tests ─────────────────────────────────────────

#[test]
fn test_own_function_single_segment() {
    let scope = build_scope("fn foo() {}");
    assert!(scope.is_own_function("foo"));
}

#[test]
fn test_own_function_not_in_scope() {
    let scope = build_scope("fn foo() {}");
    assert!(!scope.is_own_function("unknown"));
}

#[test]
fn test_own_function_type_prefix() {
    let scope = build_scope("struct Config; impl Config { fn load() {} }");
    assert!(scope.is_own_function("Config::load"));
}

#[test]
fn test_own_function_external_type() {
    let scope = build_scope("fn foo() {}");
    assert!(!scope.is_own_function("String::new"));
}

#[test]
fn test_own_function_self_inherent_is_own() {
    // Inherent impl method IS an own call (no longer blocked by UNIVERSAL_METHODS)
    let scope = build_scope("struct X; impl X { fn default(&self) {} }");
    assert!(scope.is_own_function("Self::default"));
}

#[test]
fn test_own_function_self_own_method() {
    let scope = build_scope("struct X; impl X { fn analyze(&self) {} }");
    assert!(scope.is_own_function("Self::analyze"));
}

// ── is_own_method Tests ───────────────────────────────────────────

#[test]
fn test_own_method_in_scope() {
    let scope = build_scope("struct A; impl A { fn analyze_file(&self) {} }");
    assert!(scope.is_own_method("analyze_file"));
}

#[test]
fn test_own_method_not_in_scope() {
    let scope = build_scope("struct A; impl A { fn analyze_file(&self) {} }");
    assert!(!scope.is_own_method("push"));
}

#[test]
fn test_own_method_inherent_is_own() {
    // Inherent impl constructor IS an own method (type-resolution handles disambiguation)
    let scope = build_scope("struct A; impl A { fn new() -> Self { A } }");
    assert!(scope.is_own_method("new"));
}

#[test]
fn test_own_method_not_universal() {
    let scope = build_scope("struct A; impl A { fn record_logic(&mut self) {} }");
    assert!(scope.is_own_method("record_logic"));
}

// ── Trivial Accessor Tests ───────────────────────────────────────

#[test]
fn test_trivial_method_field_access() {
    let scope = build_scope("struct S { x: i32 } impl S { fn x(&self) -> i32 { self.x } }");
    assert!(scope.trivial_methods.contains("x"));
}

#[test]
fn test_trivial_method_len_chain() {
    let scope = build_scope(
        "struct S { items: Vec<i32> } impl S { fn count(&self) -> usize { self.items.len() } }",
    );
    assert!(scope.trivial_methods.contains("count"));
}

#[test]
fn test_trivial_method_clone() {
    let scope = build_scope(
        "struct S { name: String } impl S { fn name(&self) -> String { self.name.clone() } }",
    );
    assert!(scope.trivial_methods.contains("name"));
}

#[test]
fn test_trivial_method_reference() {
    let scope = build_scope(
        "struct S { data: Vec<u8> } impl S { fn data(&self) -> &Vec<u8> { &self.data } }",
    );
    assert!(scope.trivial_methods.contains("data"));
}

#[test]
fn test_nontrivial_method_with_params() {
    let scope = build_scope(
        "struct S { v: Vec<i32> } impl S { fn get(&self, i: usize) -> i32 { self.v[i] } }",
    );
    assert!(!scope.trivial_methods.contains("get"));
}

#[test]
fn test_nontrivial_method_with_logic() {
    let scope = build_scope(
        "struct S { x: i32 } impl S { fn check(&self) -> bool { if self.x > 0 { true } else { false } } }",
    );
    assert!(!scope.trivial_methods.contains("check"));
}

#[test]
fn test_nontrivial_method_multi_stmt() {
    let scope = build_scope(
        "struct S { x: i32 } impl S { fn compute(&self) -> i32 { let y = self.x; y } }",
    );
    assert!(!scope.trivial_methods.contains("compute"));
}

#[test]
fn test_trivial_method_not_own_call() {
    let scope = build_scope(
        "struct S { items: Vec<i32> } impl S { fn count(&self) -> usize { self.items.len() } }",
    );
    assert!(
        !scope.is_own_method("count"),
        "Trivial getter should not be counted as own call"
    );
}

// ── Bug 4: Type::new() / Type::default() Not Own Call ──────────

#[test]
fn test_own_function_type_inherent_is_own() {
    // Inherent impl constructors ARE own calls (type-resolution handles disambiguation)
    let scope = build_scope("struct MyType; impl MyType { fn new() -> Self { MyType } }");
    assert!(
        scope.is_own_function("MyType::new"),
        "MyType::new() IS an own function (inherent impl)"
    );
}

#[test]
fn test_own_function_trait_impl_blocked() {
    // Trait impl methods are NOT own calls (polymorphic dispatch)
    let scope = build_scope(
        "struct MyType; trait Foo { fn foo(&self); } impl Foo for MyType { fn foo(&self) {} }",
    );
    assert!(
        !scope.is_own_function("MyType::foo"),
        "Trait impl method should NOT be own function"
    );
}

#[test]
fn test_own_function_type_custom_still_counted() {
    let scope = build_scope("struct MyType; impl MyType { fn load() -> Self { MyType } }");
    assert!(
        scope.is_own_function("MyType::load"),
        "MyType::load() SHOULD be own function (load is not universal)"
    );
}

// ── Bug 5: Trivial .get() Accessor Tests ────────────────────────

#[test]
fn test_trivial_method_get_self_field() {
    let scope = build_scope(
        "struct S { items: Vec<i32>, index: usize }
         impl S { fn current(&self) -> Option<&i32> { self.items.get(self.index) } }",
    );
    assert!(
        scope.trivial_methods.contains("current"),
        ".get(self.field) should be trivial"
    );
}

#[test]
fn test_trivial_method_get_literal() {
    let scope = build_scope(
        "struct S { items: Vec<i32> }
         impl S { fn first(&self) -> Option<&i32> { self.items.get(0) } }",
    );
    assert!(
        scope.trivial_methods.contains("first"),
        ".get(0) should be trivial"
    );
}

#[test]
fn test_trivial_method_get_ref_field() {
    let scope = build_scope(
        "struct S { map: std::collections::HashMap<String, i32>, key: String }
         impl S { fn entry(&self) -> Option<&i32> { self.map.get(&self.key) } }",
    );
    assert!(
        scope.trivial_methods.contains("entry"),
        ".get(&self.key) should be trivial"
    );
}

#[test]
fn test_trivial_method_get_deep_field() {
    let scope = build_scope(
        "struct Config { idx: usize }
         struct S { data: Vec<i32>, config: Config }
         impl S { fn item(&self) -> Option<&i32> { self.data.get(self.config.idx) } }",
    );
    assert!(
        scope.trivial_methods.contains("item"),
        ".get(self.config.idx) should be trivial"
    );
}

// ── Trait-Only Method Tests ──────────────────────────────────

#[test]
fn test_trait_only_method_not_own_call() {
    let scope = build_scope(
        "trait Provider { fn fetch(&self) -> i32; }
         struct S;
         impl Provider for S { fn fetch(&self) -> i32 { 42 } }",
    );
    assert!(
        scope.trait_only_methods.contains("fetch"),
        "fetch only appears in trait + trait impl, should be trait-only"
    );
    assert!(
        !scope.is_own_method("fetch"),
        "trait-only method should NOT be counted as own call"
    );
}

#[test]
fn test_trait_and_inherent_collision_not_trait_only() {
    let scope = build_scope(
        "trait T { fn process(&self); }
         struct A;
         impl T for A { fn process(&self) {} }
         struct B;
         impl B { fn process(&self) {} }",
    );
    assert!(
        !scope.trait_only_methods.contains("process"),
        "process appears in both trait impl and inherent impl — not trait-only"
    );
    assert!(
        scope.is_own_method("process"),
        "collision should be counted as own call"
    );
}

#[test]
fn test_external_trait_impl_trait_only() {
    // Simulates implementing an external trait locally
    let scope = build_scope(
        "struct S;
         impl std::fmt::Display for S {
             fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) }
         }",
    );
    // fmt is a UNIVERSAL_METHOD so would be excluded anyway, but the trait_only
    // tracking should still work. Let's test with a non-universal name:
    let scope2 = build_scope(
        "trait ExternalTrait { fn do_work(&self); }
         struct S;
         impl ExternalTrait for S { fn do_work(&self) {} }",
    );
    assert!(
        scope2.trait_only_methods.contains("do_work"),
        "Method only in trait impl should be trait-only"
    );
    assert!(
        !scope2.is_own_method("do_work"),
        "trait-only method should not be own call"
    );
    // Verify fmt is still in the set (even though UNIVERSAL_METHODS would block it)
    assert!(scope.trait_only_methods.contains("fmt"));
}

#[test]
fn test_self_path_trait_only_not_own_function() {
    let scope = build_scope(
        "trait Provider { fn fetch(&self) -> i32; }
         struct S;
         impl Provider for S { fn fetch(&self) -> i32 { 42 } }",
    );
    assert!(
        !scope.is_own_function("Self::fetch"),
        "Self::fetch for trait-only method should NOT be own function"
    );
}

#[test]
fn test_trait_def_only_no_impl() {
    let scope = build_scope("trait Provider { fn fetch_data(&self) -> Vec<u8>; }");
    assert!(
        scope.trait_only_methods.contains("fetch_data"),
        "Method only in trait definition should be trait-only"
    );
    assert!(
        !scope.is_own_method("fetch_data"),
        "trait-only method should not be own call"
    );
}

#[test]
fn test_nontrivial_method_get_computed_arg() {
    let scope = build_scope(
        "struct S { items: Vec<i32>, index: usize }
         impl S { fn item(&self) -> Option<&i32> { self.items.get(self.index + 1) } }",
    );
    assert!(
        !scope.trivial_methods.contains("item"),
        ".get(self.index + 1) should NOT be trivial (arithmetic)"
    );
}

// ── Name Collision Tests ─────────────────────────────────────────

#[test]
fn test_name_collision_conservative() {
    let scope = build_scope(
        "struct A { x: i32 }
         impl A { fn value(&self) -> i32 { self.x } }
         struct B { items: Vec<i32> }
         impl B { fn value(&self, idx: usize) -> i32 { self.items[idx] } }",
    );
    assert!(
        !scope.trivial_methods.contains("value"),
        "Name collision should be conservative — non-trivial wins"
    );
    assert!(
        scope.is_own_method("value"),
        "Collided method should be counted as own call"
    );
}

#[test]
fn test_enum_variant_constructor_not_own_call() {
    let code = r#"
        enum ChunkKind {
            Function,
            Method,
            Other(String),
        }
        impl ChunkKind {
            fn as_str(&self) -> &str { "" }
        }
        fn determine_kind(key: &str) -> ChunkKind {
            match key {
                "fn" => ChunkKind::Function,
                "method" => ChunkKind::Method,
                _ => ChunkKind::Other(key.to_string()),
            }
        }
    "#;
    let syntax = syn::parse_file(code).unwrap();
    let scope = ProjectScope::from_files(&[("test.rs", &syntax)]);

    // PascalCase variants should NOT be own calls
    assert!(
        !scope.is_own_function("ChunkKind::Function"),
        "Enum variant ChunkKind::Function should not be an own call"
    );
    assert!(
        !scope.is_own_function("ChunkKind::Other"),
        "Enum variant ChunkKind::Other should not be an own call"
    );
    // snake_case methods SHOULD be own calls
    assert!(
        scope.is_own_function("ChunkKind::as_str"),
        "Method ChunkKind::as_str should be an own call"
    );
}
