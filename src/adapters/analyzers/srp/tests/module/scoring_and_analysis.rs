use super::*;

#[test]
fn test_file_length_score_below_baseline() {
    let score = compute_file_length_score(100, 300, 800);
    assert!((score - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_file_length_score_at_baseline() {
    let score = compute_file_length_score(300, 300, 800);
    assert!((score - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_file_length_score_above_ceiling() {
    let score = compute_file_length_score(1000, 300, 800);
    assert!((score - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_file_length_score_midpoint() {
    let score = compute_file_length_score(550, 300, 800);
    assert!((score - 0.5).abs() < f64::EPSILON);
}

#[test]
fn test_file_length_score_at_ceiling() {
    let score = compute_file_length_score(800, 300, 800);
    assert!((score - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_analyze_module_srp_below_baseline() {
    let source = "fn foo() {}\nfn bar() {}\n";
    let syntax = syn::parse_file(source).unwrap();
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let config = SrpConfig::default(); // baseline=300
    let call_graph = HashMap::new();
    let cfg_test_files = std::collections::HashSet::new();
    let warnings = analyze_module_srp(&parsed, &config, &call_graph, &cfg_test_files, (300, 800));
    assert!(warnings.is_empty());
}

// Module-SRP file-kind matrix: `(label, fn-template (IDX → index), count, path,
// is_cfg_test, expect_flagged)`. Locks in the test-aware behaviour — file-length
// applies to test files (at the resolved test baseline), but cohesion
// (independent clusters) is production-only: independent `#[test]` fns are a
// test file's purpose, not a low-cohesion smell.
type FileKindCase = (&'static str, &'static str, usize, &'static str, bool, bool);
const FILE_KIND_CASES: &[FileKindCase] = &[
    (
        "prod over baseline → length-flagged",
        "fn func_IDX() { let x = 1; }\n",
        400,
        "big.rs",
        false,
        true,
    ),
    (
        "cfg-test over baseline → length still applies",
        "#[test]\nfn test_case_IDX() { assert!(true); }\n",
        400,
        "src/some/tests/big.rs",
        true,
        true,
    ),
    (
        "prod independent substantive fns → cohesion-flagged",
        "fn helper_IDX() { let a = 1; let b = 2; let c = 3; let d = 4; let e = 5; }\n",
        10,
        "src/prod/module.rs",
        false,
        true,
    ),
    (
        "cfg-test independent substantive fns → cohesion exempt",
        "fn helper_IDX() { let a = 1; let b = 2; let c = 3; let d = 4; let e = 5; }\n",
        10,
        "src/some/tests/helpers.rs",
        true,
        false,
    ),
];

#[test]
fn test_analyze_module_srp_length_and_cohesion_by_file_kind() {
    for (label, template, count, path, is_cfg_test, expect_flagged) in FILE_KIND_CASES {
        let mut source = String::new();
        for i in 0..*count {
            source.push_str(&template.replace("IDX", &i.to_string()));
        }
        let syntax = syn::parse_file(&source).unwrap();
        let parsed = vec![(path.to_string(), source.clone(), syntax)];
        let mut cfg_test_files = std::collections::HashSet::new();
        if *is_cfg_test {
            cfg_test_files.insert(path.to_string());
        }
        let warnings = analyze_module_srp(
            &parsed,
            &SrpConfig::default(),
            &HashMap::new(),
            &cfg_test_files,
            (300, 800),
        );
        assert_eq!(
            !warnings.is_empty(),
            *expect_flagged,
            "case {label}: {warnings:?}"
        );
    }
}

#[test]
fn test_analyze_module_srp_test_lines_excluded() {
    // Production lines below baseline, but with large test module
    let mut source = String::from("fn foo() {}\nfn bar() {}\n\n#[cfg(test)]\nmod tests {\n");
    for i in 0..500 {
        source.push_str(&format!("    fn test_{i}() {{ assert!(true); }}\n"));
    }
    source.push_str("}\n");
    let syntax = syn::parse_file(&source).unwrap();
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let config = SrpConfig::default();
    let call_graph = HashMap::new();
    let cfg_test_files = std::collections::HashSet::new();
    let warnings = analyze_module_srp(&parsed, &config, &call_graph, &cfg_test_files, (300, 800));
    assert!(
        warnings.is_empty(),
        "Test code should not count towards production lines"
    );
}

// ── Free function collector tests ─────────────────────────────

#[test]
fn test_collect_free_functions_basic() {
    let code = "fn foo() {} pub fn bar() {} fn baz(x: i32) { let a = 1; let b = 2; }";
    let syntax = syn::parse_file(code).unwrap();
    let fns = collect_free_functions(&syntax);
    assert_eq!(fns.len(), 3);
    assert!(fns[0].is_private);
    assert!(!fns[1].is_private);
    assert!(fns[2].is_private);
    assert_eq!(fns[2].statement_count, 2);
}

#[test]
fn test_collect_free_functions_skips_impl_methods() {
    let code = "struct S; impl S { fn method(&self) {} } fn free() {}";
    let syntax = syn::parse_file(code).unwrap();
    let fns = collect_free_functions(&syntax);
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "free");
}
