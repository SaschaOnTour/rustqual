use std::collections::HashMap;

use syn::visit::Visit;

use crate::config::sections::SrpConfig;

use super::union_find::UnionFind;
use super::ModuleSrpWarning;

/// Information about a free (non-method) function collected from the AST.
struct FreeFunctionInfo {
    name: String,
    is_private: bool,
    statement_count: usize,
}

/// AST visitor that collects free function metadata for cohesion analysis.
struct FreeFunctionCollector<'a> {
    functions: &'a mut Vec<FreeFunctionInfo>,
}

impl<'ast, 'a> Visit<'ast> for FreeFunctionCollector<'a> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.functions.push(FreeFunctionInfo {
            name: node.sig.ident.to_string(),
            is_private: matches!(node.vis, syn::Visibility::Inherited),
            statement_count: node.block.stmts.len(),
        });
        // Don't recurse into body — we only need function-level info
    }

    // Don't descend into impl blocks or nested modules
    fn visit_item_impl(&mut self, _node: &'ast syn::ItemImpl) {}
    fn visit_item_mod(&mut self, _node: &'ast syn::ItemMod) {}
    fn visit_item_trait(&mut self, _node: &'ast syn::ItemTrait) {}
}

/// Collect free functions from a parsed syntax tree.
/// Operation: creates visitor and walks items.
fn collect_free_functions(syntax: &syn::File) -> Vec<FreeFunctionInfo> {
    let mut functions = Vec::new();
    let mut collector = FreeFunctionCollector {
        functions: &mut functions,
    };
    collector.visit_file(syntax);
    functions
}

/// Count independent function clusters in a file using Union-Find.
/// Operation: Union-Find on private substantive functions using call graph.
fn count_independent_clusters(
    fn_info: &[FreeFunctionInfo],
    call_graph: &[(String, Vec<String>)],
    min_statements: usize,
) -> (usize, Vec<Vec<String>>) {
    let substantive: Vec<&FreeFunctionInfo> = fn_info
        .iter()
        .filter(|f| f.is_private && f.statement_count >= min_statements)
        .collect();
    if substantive.is_empty() {
        return (0, vec![]);
    }
    let name_to_idx: HashMap<&str, usize> = substantive
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.as_str(), i))
        .collect();
    let make_uf = |size| UnionFind::new(size);
    let mut uf = make_uf(substantive.len());
    let unite = |uf: &mut UnionFind, a: usize, b: usize| uf.union(a, b);
    let components = |uf: &mut UnionFind| uf.component_members();
    // Union-Find: unite private targets per caller + connect caller if private
    call_graph.iter().for_each(|(fn_name, targets)| {
        let private_targets: Vec<usize> = targets
            .iter()
            .filter_map(|t| name_to_idx.get(t.as_str()).copied())
            .collect();
        (1..private_targets.len()).for_each(|i| {
            unite(&mut uf, private_targets[0], private_targets[i]);
        });
        if let Some(&caller_idx) = name_to_idx.get(fn_name.as_str()) {
            if let Some(&first) = private_targets.first() {
                unite(&mut uf, caller_idx, first);
            }
        }
    });
    let component_members = components(&mut uf);
    let mut cluster_names: Vec<Vec<String>> = component_members
        .values()
        .map(|indices| {
            indices
                .iter()
                .map(|&i| substantive[i].name.clone())
                .collect()
        })
        .collect();
    cluster_names.iter_mut().for_each(|c| c.sort());
    cluster_names.sort();
    let count = cluster_names.len();
    (count, cluster_names)
}

/// Analyze module-level SRP: flag files with excessive production line counts
/// or too many independent function clusters.
/// Operation: iterates files, computes production lines, length score,
/// and independent clusters via closures.
pub fn analyze_module_srp(
    parsed: &[(String, String, syn::File)],
    config: &SrpConfig,
    file_call_graph: &HashMap<String, Vec<(String, Vec<String>)>>,
    cfg_test_files: &std::collections::HashSet<String>,
) -> Vec<ModuleSrpWarning> {
    parsed
        .iter()
        .filter(|(path, _, _)| !cfg_test_files.contains(path))
        .filter_map(|(path, source, syntax)| {
            let production_lines = count_production_lines(source);
            let score = compute_file_length_score(
                production_lines,
                config.file_length_baseline,
                config.file_length_ceiling,
            );

            let free_fns = collect_free_functions(syntax);
            let call_graph = file_call_graph
                .get(path)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let (cluster_count, cluster_names) =
                count_independent_clusters(&free_fns, call_graph, config.min_cluster_statements);

            let has_length_warning = score > 0.0;
            let has_cohesion_warning = cluster_count >= config.max_independent_clusters;

            if has_length_warning || has_cohesion_warning {
                Some(ModuleSrpWarning {
                    module: path.clone(),
                    file: path.clone(),
                    production_lines,
                    length_score: score,
                    independent_clusters: cluster_count,
                    cluster_names,
                    suppressed: false,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Count production lines: lines from start of file to first `#[cfg(test)]` module.
/// Operation: string scanning logic, no own calls.
fn count_production_lines(source: &str) -> usize {
    let mut count = 0;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed == "#[cfg(test)]" {
            break;
        }
        // Skip blank lines and pure comment lines
        if !trimmed.is_empty() && !trimmed.starts_with("//") {
            count += 1;
        }
    }
    count
}

/// Compute file length penalty score.
/// Returns 0.0 below baseline, 1.0 above ceiling, linear between.
/// Operation: arithmetic.
fn compute_file_length_score(production_lines: usize, baseline: usize, ceiling: usize) -> f64 {
    if production_lines <= baseline {
        return 0.0;
    }
    if production_lines >= ceiling {
        return 1.0;
    }
    let range = (ceiling - baseline) as f64;
    if range <= 0.0 {
        return 1.0;
    }
    (production_lines - baseline) as f64 / range
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_production_lines_simple() {
        let source = "fn main() {\n    println!(\"hello\");\n}\n";
        assert_eq!(count_production_lines(source), 3);
    }

    #[test]
    fn test_count_production_lines_with_test_module() {
        let source =
            "fn main() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn test_it() {}\n}\n";
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
        let config = SrpConfig {
            max_independent_clusters: 3,
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
}
