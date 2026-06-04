use super::*;

#[test]
fn test_new_defaults() {
    let config = Config::default();
    let scope = empty_scope();
    let visitor = BodyVisitor::new(&config, &scope, Some("test_fn"), None, HashMap::new());
    assert!(visitor.logic.is_empty());
    assert!(visitor.own_calls.is_empty());
    assert_eq!(visitor.max_nesting, 0);
    assert_eq!(visitor.closure_depth, 0);
    assert_eq!(visitor.async_block_depth, 0);
    assert_eq!(visitor.nesting_depth, 0);
    assert_eq!(visitor.current_fn_name, Some("test_fn".to_string()));
    assert_eq!(visitor.cognitive_complexity, 0);
    assert_eq!(visitor.cyclomatic_complexity, 1); // base path
    assert!(visitor.complexity_hotspots.is_empty());
    assert!(visitor.magic_numbers.is_empty());
    assert!(visitor.last_boolean_op.is_none());
    assert_eq!(visitor.in_const_context, 0);
}

#[test]
fn test_new_without_fn_name() {
    let config = Config::default();
    let scope = empty_scope();
    let visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
    assert!(visitor.current_fn_name.is_none());
}

#[test]
fn test_in_lenient_nested_context_closure() {
    let config = Config::default();
    let scope = empty_scope();
    let mut visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
    assert!(!visitor.in_lenient_nested_context());
    visitor.closure_depth = 1;
    assert!(visitor.in_lenient_nested_context());
}

#[test]
fn test_in_lenient_nested_context_strict_mode() {
    let mut config = Config::default();
    config.strict_closures = true;
    let scope = empty_scope();
    let mut visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
    visitor.closure_depth = 1;
    assert!(!visitor.in_lenient_nested_context());
}

#[test]
fn test_in_lenient_nested_context_async_block() {
    let config = Config::default();
    let scope = empty_scope();
    let mut visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
    visitor.async_block_depth = 1;
    assert!(visitor.in_lenient_nested_context());
}

#[test]
fn test_is_iterator_method_known() {
    assert!(BodyVisitor::is_iterator_method("map"));
    assert!(BodyVisitor::is_iterator_method("filter"));
    assert!(BodyVisitor::is_iterator_method("collect"));
    assert!(BodyVisitor::is_iterator_method("fold"));
    assert!(BodyVisitor::is_iterator_method("iter"));
    assert!(BodyVisitor::is_iterator_method("into_iter"));
}

#[test]
fn test_is_iterator_method_unknown() {
    assert!(!BodyVisitor::is_iterator_method("foo"));
    assert!(!BodyVisitor::is_iterator_method("bar"));
    assert!(!BodyVisitor::is_iterator_method("push"));
    assert!(!BodyVisitor::is_iterator_method("analyze"));
}

#[test]
fn test_is_recursive_call_match() {
    let config = Config::default();
    let scope = empty_scope();
    let visitor = BodyVisitor::new(&config, &scope, Some("my_func"), None, HashMap::new());
    assert!(visitor.is_recursive_call("my_func"));
}

#[test]
fn test_is_recursive_call_qualified() {
    let config = Config::default();
    let scope = empty_scope();
    let visitor = BodyVisitor::new(&config, &scope, Some("bar"), None, HashMap::new());
    assert!(visitor.is_recursive_call("Foo::bar"));
}

#[test]
fn test_is_recursive_call_no_fn_name() {
    let config = Config::default();
    let scope = empty_scope();
    let visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
    assert!(!visitor.is_recursive_call("anything"));
}

#[test]
fn test_enter_exit_nesting() {
    let config = Config::default();
    let scope = empty_scope();
    let mut visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
    assert_eq!(visitor.nesting_depth, 0);
    assert_eq!(visitor.max_nesting, 0);

    visitor.enter_nesting();
    assert_eq!(visitor.nesting_depth, 1);
    assert_eq!(visitor.max_nesting, 1);

    visitor.enter_nesting();
    assert_eq!(visitor.nesting_depth, 2);
    assert_eq!(visitor.max_nesting, 2);

    visitor.exit_nesting();
    assert_eq!(visitor.nesting_depth, 1);
    assert_eq!(visitor.max_nesting, 2);

    visitor.exit_nesting();
    assert_eq!(visitor.nesting_depth, 0);
    assert_eq!(visitor.max_nesting, 2);
}

#[test]
fn test_extract_call_name_path() {
    let expr: syn::Expr = syn::parse_quote!(foo::bar);
    assert_eq!(
        BodyVisitor::extract_call_name(&expr),
        Some("foo::bar".to_string())
    );
}

#[test]
fn test_extract_call_name_simple() {
    let expr: syn::Expr = syn::parse_quote!(my_func);
    assert_eq!(
        BodyVisitor::extract_call_name(&expr),
        Some("my_func".to_string())
    );
}

#[test]
fn test_extract_call_name_non_path() {
    let expr: syn::Expr = syn::parse_quote!(42);
    assert_eq!(BodyVisitor::extract_call_name(&expr), None);
}

#[test]
fn test_record_logic_normal() {
    let config = Config::default();
    let scope = empty_scope();
    let mut visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
    visitor.record_logic("if", proc_macro2::Span::call_site());
    assert_eq!(visitor.logic.len(), 1);
    assert_eq!(visitor.logic[0].kind, "if");
}

#[test]
fn test_record_logic_skipped_in_closure() {
    let config = Config::default();
    let scope = empty_scope();
    let mut visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
    visitor.closure_depth = 1;
    visitor.record_logic("if", proc_macro2::Span::call_site());
    assert!(visitor.logic.is_empty());
}

#[test]
fn test_record_logic_in_for_iter() {
    let config = Config::default();
    let scope = empty_scope();
    let mut visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
    visitor.in_for_iter = true;
    visitor.record_logic("comparison", proc_macro2::Span::call_site());
    assert!(visitor.logic.is_empty());
}

#[test]
fn test_record_logic_in_async_block_lenient() {
    let config = Config::default();
    let scope = empty_scope();
    let mut visitor = BodyVisitor::new(&config, &scope, None, None, HashMap::new());
    visitor.async_block_depth = 1;
    visitor.record_logic("if", proc_macro2::Span::call_site());
    assert!(visitor.logic.is_empty());
}
