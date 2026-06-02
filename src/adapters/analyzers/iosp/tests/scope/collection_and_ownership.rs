use super::*;

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
