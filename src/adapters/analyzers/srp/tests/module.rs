use crate::adapters::analyzers::srp::module::*;
use crate::config::sections::SrpConfig;
use std::collections::HashMap;
use syn::visit::Visit;

#[test]
fn test_count_production_lines_simple() {
    let source = "fn main() {\n    println!(\"hello\");\n}\n";
    assert_eq!(count_production_lines(source), 3);
}

#[test]
fn test_count_production_lines_with_test_module() {
    let source = "fn main() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn test_it() {}\n}\n";
    // Only "fn main() {}" is production code (1 line of code, blank lines skipped)
    assert_eq!(count_production_lines(source), 1);
}

#[test]
fn test_count_production_lines_skips_comments() {
    let source = "// This is a comment\nfn foo() {}\n// Another comment\nfn bar() {}\n";
    assert_eq!(count_production_lines(source), 2);
}

#[test]
fn test_count_production_lines_skips_blanks() {
    let source = "\n\nfn foo() {}\n\n\nfn bar() {}\n\n";
    assert_eq!(count_production_lines(source), 2);
}

#[test]
fn test_count_production_lines_stops_on_single_line_cfg_test() {
    // Single-line form: `#[cfg(test)] mod tests { ... }` on one line.
    // Previously this would not match `trimmed == "#[cfg(test)]"` and
    // the test body would be counted as production.
    let source = "fn main() {}\n#[cfg(test)] mod tests { fn t() {} fn u() {} }\n";
    assert_eq!(
        count_production_lines(source),
        1,
        "single-line `#[cfg(test)] mod tests` must terminate counting"
    );
}

#[test]
fn test_count_production_lines_stops_on_cfg_test_with_trailing_whitespace() {
    // `#[cfg(test)]    ` (trailing whitespace) — exact-equality check
    // used to skip past this line.
    let source = "fn main() {}\n#[cfg(test)]   \nmod tests { fn t() {} }\n";
    assert_eq!(count_production_lines(source), 1);
}

#[test]
fn test_count_production_lines_skips_block_comment() {
    // Multi-line `/* … */` block: opening line, body lines, and
    // closing line are all non-production.
    let source = "\
/*
 * A multi-line block comment.
 * Spans several lines.
 */
fn foo() {}
";
    assert_eq!(
        count_production_lines(source),
        1,
        "only `fn foo() {{}}` is production"
    );
}

#[test]
fn test_count_production_lines_skips_single_line_block_comment() {
    let source = "/* header */\nfn foo() {}\nfn bar() {}\n";
    assert_eq!(count_production_lines(source), 2);
}

#[test]
fn test_count_production_lines_counts_deref_starting_lines() {
    // Lines starting with `*` outside a block comment are valid code
    // (deref / assign-through pointer) and must count as production.
    let source = "\
fn write(p: &mut i32) {
    *p = 42;
    *p += 1;
}
";
    // Body lines: `fn` signature, 2 deref assignments, closing `}` — 4.
    assert_eq!(count_production_lines(source), 4);
}

#[test]
fn test_count_production_lines_counts_code_before_block_comment() {
    // `let x = 1; /* note */` has code before the comment and must count.
    let source = "fn foo() {\n    let x = 1; /* note */\n}\n";
    assert_eq!(count_production_lines(source), 3);
}

#[test]
fn test_count_production_lines_counts_code_after_inline_block_comment() {
    // `/* note */ let x = 1;` has code AFTER the inline comment.
    // A leading-only heuristic would wrongly skip this.
    let source = "fn foo() {\n    /* note */ let x = 1;\n}\n";
    assert_eq!(count_production_lines(source), 3);
}

#[test]
fn test_count_production_lines_skips_pure_inline_block_comment() {
    let source = "fn foo() {\n    /* note */\n}\n";
    assert_eq!(count_production_lines(source), 2);
}

#[test]
fn test_count_production_lines_handles_nested_block_comments() {
    // Rust supports nested block comments: the inner `*/` must NOT
    // close the outer, so "still outer" stays inside the comment.
    let source = "\
/* outer
   /* inner */
   still outer */
fn foo() {}
";
    assert_eq!(
        count_production_lines(source),
        1,
        "nested block comments must track depth, not a boolean flag"
    );
}

#[test]
fn test_count_production_lines_nested_block_closes_properly() {
    // Confirm depth unwinds correctly: after both `*/` the scanner
    // is back at depth 0 and recognises `fn bar() {}` on the same
    // line as real code.
    let source = "/* a /* b */ c */ fn bar() {}\n";
    assert_eq!(count_production_lines(source), 1);
}

#[test]
fn test_count_production_lines_empty() {
    assert_eq!(count_production_lines(""), 0);
}

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
    let warnings = analyze_module_srp(&parsed, &config, &call_graph, &cfg_test_files);
    assert!(warnings.is_empty());
}

#[test]
fn test_analyze_module_srp_above_baseline() {
    // Generate source with many lines
    let mut source = String::new();
    for i in 0..400 {
        source.push_str(&format!("fn func_{i}() {{ let x = 1; }}\n"));
    }
    let syntax = syn::parse_file(&source).unwrap();
    let parsed = vec![("big.rs".to_string(), source.to_string(), syntax)];
    let config = SrpConfig::default();
    let call_graph = HashMap::new();
    let cfg_test_files = std::collections::HashSet::new();
    let warnings = analyze_module_srp(&parsed, &config, &call_graph, &cfg_test_files);
    assert!(!warnings.is_empty());
    assert_eq!(warnings[0].module, "big.rs");
    assert!(warnings[0].length_score > 0.0);
}

#[test]
fn test_analyze_module_srp_skips_cfg_test_files() {
    // A file reachable only under `#[cfg(test)]` is exempt from the
    // module-SRP check. Without this, test-helper files with many
    // independent #[test] fns get falsely flagged as "too many
    // independent clusters" or "too many production lines".
    let mut source = String::new();
    for i in 0..10 {
        source.push_str(&format!(
            "#[test]\nfn test_scenario_{i}() {{ assert!(true); }}\n"
        ));
    }
    let syntax = syn::parse_file(&source).unwrap();
    let parsed = vec![(
        "src/some/tests/helpers.rs".to_string(),
        source.to_string(),
        syntax,
    )];
    let config = SrpConfig::default();
    let call_graph = HashMap::new();
    let mut cfg_test_files = std::collections::HashSet::new();
    cfg_test_files.insert("src/some/tests/helpers.rs".to_string());
    let warnings = analyze_module_srp(&parsed, &config, &call_graph, &cfg_test_files);
    assert!(
        warnings.is_empty(),
        "cfg-test file must be skipped: {warnings:?}"
    );
}

#[test]
fn test_analyze_module_srp_still_flags_non_cfg_test_files() {
    // Negative control: without cfg-test tag, a big production file with
    // many isolated substantive functions is flagged as "too many clusters".
    let mut source = String::new();
    for i in 0..10 {
        source.push_str(&format!(
            "fn helper_{i}() {{ let a = 1; let b = 2; let c = 3; let d = 4; let e = 5; }}\n"
        ));
    }
    let syntax = syn::parse_file(&source).unwrap();
    let parsed = vec![("src/prod/module.rs".to_string(), source.to_string(), syntax)];
    let config = SrpConfig::default();
    let call_graph = HashMap::new();
    let cfg_test_files = std::collections::HashSet::new(); // empty
    let warnings = analyze_module_srp(&parsed, &config, &call_graph, &cfg_test_files);
    assert!(
        !warnings.is_empty(),
        "production file with many unconnected substantive fns must be flagged"
    );
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
    let warnings = analyze_module_srp(&parsed, &config, &call_graph, &cfg_test_files);
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

// ── Independent cluster tests ─────────────────────────────────

#[test]
fn test_clusters_no_functions() {
    let (count, names) = count_independent_clusters(&[], &[], 5);
    assert_eq!(count, 0);
    assert!(names.is_empty());
}

#[test]
fn test_clusters_single_private_function() {
    let fns = vec![FreeFunctionInfo {
        name: "alpha".to_string(),
        is_private: true,
        statement_count: 10,
    }];
    let (count, _) = count_independent_clusters(&fns, &[], 5);
    assert_eq!(count, 1);
}

#[test]
fn test_clusters_connected_functions() {
    let fns = vec![
        FreeFunctionInfo {
            name: "a".to_string(),
            is_private: true,
            statement_count: 10,
        },
        FreeFunctionInfo {
            name: "b".to_string(),
            is_private: true,
            statement_count: 10,
        },
        FreeFunctionInfo {
            name: "c".to_string(),
            is_private: true,
            statement_count: 10,
        },
    ];
    // a calls b, b calls c → all connected
    let calls = vec![
        ("a".to_string(), vec!["b".to_string()]),
        ("b".to_string(), vec!["c".to_string()]),
    ];
    let (count, names) = count_independent_clusters(&fns, &calls, 5);
    assert_eq!(count, 1);
    assert_eq!(names[0].len(), 3);
}

#[test]
fn test_clusters_disconnected_functions() {
    let fns = vec![
        FreeFunctionInfo {
            name: "a".to_string(),
            is_private: true,
            statement_count: 10,
        },
        FreeFunctionInfo {
            name: "b".to_string(),
            is_private: true,
            statement_count: 10,
        },
        FreeFunctionInfo {
            name: "c".to_string(),
            is_private: true,
            statement_count: 10,
        },
        FreeFunctionInfo {
            name: "d".to_string(),
            is_private: true,
            statement_count: 10,
        },
    ];
    // a calls b, c calls d → 2 clusters
    let calls = vec![
        ("a".to_string(), vec!["b".to_string()]),
        ("c".to_string(), vec!["d".to_string()]),
    ];
    let (count, names) = count_independent_clusters(&fns, &calls, 5);
    assert_eq!(count, 2);
    assert_eq!(names.len(), 2);
}

#[test]
fn test_clusters_public_functions_excluded() {
    let fns = vec![
        FreeFunctionInfo {
            name: "pub_fn".to_string(),
            is_private: false,
            statement_count: 10,
        },
        FreeFunctionInfo {
            name: "priv_fn".to_string(),
            is_private: true,
            statement_count: 10,
        },
    ];
    let (count, _) = count_independent_clusters(&fns, &[], 5);
    assert_eq!(count, 1); // only priv_fn counted
}

#[test]
fn test_clusters_small_functions_excluded() {
    let fns = vec![
        FreeFunctionInfo {
            name: "small".to_string(),
            is_private: true,
            statement_count: 2,
        },
        FreeFunctionInfo {
            name: "big".to_string(),
            is_private: true,
            statement_count: 10,
        },
    ];
    let (count, _) = count_independent_clusters(&fns, &[], 5);
    assert_eq!(count, 1); // only big counted
}

#[test]
fn test_clusters_three_independent_triggers_warning() {
    let fns = vec![
        FreeFunctionInfo {
            name: "algo1".to_string(),
            is_private: true,
            statement_count: 10,
        },
        FreeFunctionInfo {
            name: "algo2".to_string(),
            is_private: true,
            statement_count: 10,
        },
        FreeFunctionInfo {
            name: "algo3".to_string(),
            is_private: true,
            statement_count: 10,
        },
    ];
    // No calls between them → 3 independent clusters
    let (count, names) = count_independent_clusters(&fns, &[], 5);
    assert_eq!(count, 3);
    assert_eq!(names.len(), 3);
}

#[test]
fn test_clusters_shared_caller_unites_callees() {
    // Private functions a, b, c are all called by public entry_point
    // → they serve the same responsibility and should be 1 cluster
    let fns = vec![
        FreeFunctionInfo {
            name: "a".to_string(),
            is_private: true,
            statement_count: 10,
        },
        FreeFunctionInfo {
            name: "b".to_string(),
            is_private: true,
            statement_count: 10,
        },
        FreeFunctionInfo {
            name: "c".to_string(),
            is_private: true,
            statement_count: 10,
        },
    ];
    // entry_point (not in private set) calls a, b, c → unites them
    let calls = vec![(
        "entry_point".to_string(),
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
    )];
    let (count, names) = count_independent_clusters(&fns, &calls, 5);
    assert_eq!(count, 1);
    assert_eq!(names[0].len(), 3);
}

#[test]
fn test_clusters_two_callers_two_groups() {
    // Two public entry points each calling different private functions
    // → 2 clusters, not 4
    let fns = vec![
        FreeFunctionInfo {
            name: "a".to_string(),
            is_private: true,
            statement_count: 10,
        },
        FreeFunctionInfo {
            name: "b".to_string(),
            is_private: true,
            statement_count: 10,
        },
        FreeFunctionInfo {
            name: "c".to_string(),
            is_private: true,
            statement_count: 10,
        },
        FreeFunctionInfo {
            name: "d".to_string(),
            is_private: true,
            statement_count: 10,
        },
    ];
    let calls = vec![
        ("pub1".to_string(), vec!["a".to_string(), "b".to_string()]),
        ("pub2".to_string(), vec!["c".to_string(), "d".to_string()]),
    ];
    let (count, names) = count_independent_clusters(&fns, &calls, 5);
    assert_eq!(count, 2);
    assert_eq!(names.len(), 2);
}

#[test]
fn test_cohesion_warning_without_length_warning() {
    // File is short (below baseline) but has 3+ independent private algorithms
    let code = r#"
fn algo_sort(data: &mut [i32]) {
let n = data.len();
let mut swapped = true;
while swapped {
    swapped = false;
    for i in 1..n {
        if data[i - 1] > data[i] {
            data.swap(i - 1, i);
            swapped = true;
        }
    }
}
}
fn algo_search(data: &[i32], target: i32) -> Option<usize> {
let mut lo = 0;
let mut hi = data.len();
while lo < hi {
    let mid = (lo + hi) / 2;
    if data[mid] == target {
        return Some(mid);
    } else if data[mid] < target {
        lo = mid + 1;
    } else {
        hi = mid;
    }
}
None
}
fn algo_hash(data: &[u8]) -> u64 {
let mut h: u64 = 0;
for &b in data {
    h = h.wrapping_mul(31).wrapping_add(b as u64);
}
let extra = data.len() as u64;
let final_val = h ^ extra;
final_val
}
"#;
    let syntax = syn::parse_file(code).unwrap();
    let parsed = vec![("algos.rs".to_string(), code.to_string(), syntax)];
    // `max_*` thresholds are exclusive ("highest value that still
    // passes"); with 3 independent clusters in the fixture, setting
    // the max to 2 is what triggers the warning.
    let config = SrpConfig {
        max_independent_clusters: 2,
        min_cluster_statements: 3,
        ..SrpConfig::default()
    };
    let call_graph = HashMap::new();
    let cfg_test_files = std::collections::HashSet::new();
    let warnings = analyze_module_srp(&parsed, &config, &call_graph, &cfg_test_files);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].independent_clusters, 3);
    assert!((warnings[0].length_score - 0.0).abs() < f64::EPSILON);
}
