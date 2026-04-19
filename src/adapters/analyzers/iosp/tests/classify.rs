use crate::adapters::analyzers::iosp::classify::*;
use crate::adapters::analyzers::iosp::scope::ProjectScope;
use crate::adapters::analyzers::iosp::types::{
    CallOccurrence, Classification, ComplexityMetrics, LogicOccurrence,
};
use crate::config::Config;
use syn::ItemImpl;

#[test]
fn test_is_trivial_body_empty() {
    let block: syn::Block = syn::parse_quote!({});
    assert!(is_trivial_body(&block).is_some());
}

#[test]
fn test_is_trivial_body_single_expr_now_analyzed() {
    // A5: single-expression bodies are now analyzed by BodyVisitor
    let block: syn::Block = syn::parse_quote!({ 42 });
    assert!(
        is_trivial_body(&block).is_none(),
        "Single-expr body should not be trivially skipped"
    );
}

#[test]
fn test_is_trivial_body_multiple() {
    let block: syn::Block = syn::parse_quote!({
        let x = 1;
        let y = 2;
    });
    assert!(is_trivial_body(&block).is_none());
}

#[test]
fn test_classify_from_findings_integration() {
    let logic = vec![];
    let own_calls = vec![CallOccurrence {
        name: "helper".to_string(),
        line: 1,
    }];
    assert_eq!(
        classify_from_findings(logic, own_calls),
        Classification::Integration
    );
}

#[test]
fn test_classify_from_findings_operation() {
    let logic = vec![LogicOccurrence {
        kind: "if".to_string(),
        line: 1,
    }];
    let own_calls = vec![];
    assert_eq!(
        classify_from_findings(logic, own_calls),
        Classification::Operation
    );
}

#[test]
fn test_classify_from_findings_violation() {
    let logic = vec![LogicOccurrence {
        kind: "if".to_string(),
        line: 1,
    }];
    let own_calls = vec![CallOccurrence {
        name: "helper".to_string(),
        line: 2,
    }];
    let result = classify_from_findings(logic, own_calls);
    assert!(
        matches!(result, Classification::Violation { .. }),
        "Expected Violation, got {:?}",
        result
    );
}

#[test]
fn test_classify_from_findings_trivial() {
    let result = classify_from_findings(vec![], vec![]);
    assert_eq!(result, Classification::Trivial);
}

#[test]
fn test_dedup_calls_no_duplicates() {
    let calls = vec![
        CallOccurrence {
            name: "a".to_string(),
            line: 1,
        },
        CallOccurrence {
            name: "b".to_string(),
            line: 2,
        },
        CallOccurrence {
            name: "c".to_string(),
            line: 3,
        },
    ];
    let result = dedup_calls(calls);
    assert_eq!(result.len(), 3);
}

#[test]
fn test_dedup_calls_with_duplicates() {
    let calls = vec![
        CallOccurrence {
            name: "a".to_string(),
            line: 1,
        },
        CallOccurrence {
            name: "b".to_string(),
            line: 2,
        },
        CallOccurrence {
            name: "a".to_string(),
            line: 5,
        },
    ];
    let result = dedup_calls(calls);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].name, "a");
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].name, "b");
}

#[test]
fn test_dedup_calls_empty() {
    let result = dedup_calls(vec![]);
    assert!(result.is_empty());
}

#[test]
fn test_attach_metrics_non_trivial() {
    let metrics = ComplexityMetrics {
        logic_count: 2,
        call_count: 1,
        max_nesting: 1,
        ..Default::default()
    };
    let expected = metrics.clone();
    let (class, m) = attach_metrics(Classification::Operation, metrics);
    assert_eq!(class, Classification::Operation);
    assert_eq!(m, Some(expected));
}

#[test]
fn test_attach_metrics_trivial() {
    let metrics = ComplexityMetrics::default();
    let (class, m) = attach_metrics(Classification::Trivial, metrics);
    assert_eq!(class, Classification::Trivial);
    assert!(m.is_none());
}

#[test]
fn test_build_classification_result_operation() {
    let logic = vec![LogicOccurrence {
        kind: "if".to_string(),
        line: 1,
    }];
    let metrics = ComplexityMetrics {
        logic_count: 1,
        call_count: 0,
        max_nesting: 1,
        cognitive_complexity: 1,
        cyclomatic_complexity: 2,
        ..Default::default()
    };
    let (class, result_metrics, own_calls) = build_classification_result(logic, vec![], metrics);
    assert_eq!(class, Classification::Operation);
    assert!(own_calls.is_empty());
    let m = result_metrics.unwrap();
    assert_eq!(m.logic_count, 1);
    assert_eq!(m.call_count, 0);
    assert_eq!(m.max_nesting, 1);
    assert_eq!(m.cognitive_complexity, 1);
    assert_eq!(m.cyclomatic_complexity, 2);
}

#[test]
fn test_build_classification_result_trivial() {
    let metrics = ComplexityMetrics {
        cyclomatic_complexity: 1,
        ..Default::default()
    };
    let (class, result_metrics, _own_calls) = build_classification_result(vec![], vec![], metrics);
    assert_eq!(class, Classification::Trivial);
    assert!(result_metrics.is_none());
}

#[test]
fn test_build_classification_result_returns_call_names() {
    let calls = vec![
        CallOccurrence {
            name: "alpha".to_string(),
            line: 1,
        },
        CallOccurrence {
            name: "beta".to_string(),
            line: 2,
        },
        CallOccurrence {
            name: "alpha".to_string(),
            line: 3,
        },
    ];
    let metrics = ComplexityMetrics {
        call_count: 3,
        cyclomatic_complexity: 1,
        ..Default::default()
    };
    let (_class, _metrics, own_calls) = build_classification_result(vec![], calls, metrics);
    assert_eq!(own_calls, vec!["alpha", "beta"]);
}

#[test]
fn test_extract_type_name_simple() {
    let item: ItemImpl = syn::parse_quote! { impl Foo {} };
    assert_eq!(extract_type_name(&item), Some("Foo".to_string()));
}

#[test]
fn test_extract_type_name_generic() {
    let item: ItemImpl = syn::parse_quote! { impl Foo<T> {} };
    let name = extract_type_name(&item).unwrap();
    assert!(
        name.starts_with("Foo<"),
        "Expected 'Foo<...>', got '{name}'"
    );
    assert!(name.contains('T'), "Expected type param T in '{name}'");
}

#[test]
fn test_extract_type_name_no_path() {
    let mut item: ItemImpl = syn::parse_quote! { impl Foo {} };
    // Replace self_ty with a non-Path type (tuple) to trigger the None case
    *item.self_ty = syn::Type::Tuple(syn::TypeTuple {
        paren_token: syn::token::Paren::default(),
        elems: syn::punctuated::Punctuated::new(),
    });
    assert_eq!(extract_type_name(&item), None);
}

#[test]
fn test_for_loop_delegation_is_integration() {
    let code = r#"
        fn process(_x: i32) {}
        fn f(items: Vec<i32>) {
            for x in items {
                process(x);
            }
        }
    "#;
    let syntax = syn::parse_file(code).unwrap();
    let scope = ProjectScope::from_files(&[("test.rs", &syntax)]);
    let config = Config::default();
    let f_fn = syntax
        .items
        .iter()
        .find_map(|item| {
            if let syn::Item::Fn(f) = item {
                if f.sig.ident == "f" {
                    return Some(f);
                }
            }
            None
        })
        .unwrap();
    let (class, _, _) = classify_function(&f_fn.block, &config, &scope, "f", (None, &f_fn.sig));
    assert_eq!(
        class,
        Classification::Integration,
        "For-loop with delegation-only body should be Integration, got {:?}",
        class
    );
}

#[test]
fn test_for_loop_with_logic_is_violation() {
    let code = r#"
        fn process(_x: i32) {}
        fn f(items: Vec<i32>) {
            for x in items {
                if x > 0 {
                    process(x);
                }
            }
        }
    "#;
    let syntax = syn::parse_file(code).unwrap();
    let scope = ProjectScope::from_files(&[("test.rs", &syntax)]);
    let config = Config::default();
    let f_fn = syntax
        .items
        .iter()
        .find_map(|item| {
            if let syn::Item::Fn(f) = item {
                if f.sig.ident == "f" {
                    return Some(f);
                }
            }
            None
        })
        .unwrap();
    let (class, _, _) = classify_function(&f_fn.block, &config, &scope, "f", (None, &f_fn.sig));
    assert!(
        matches!(class, Classification::Violation { .. }),
        "For-loop with logic should be Violation, got {:?}",
        class
    );
}
