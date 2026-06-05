use super::*;

/// Three independent private algorithms (sort / search / hash) with no calls
/// between them — a short file with 3 cohesion clusters but well under the
/// file_length threshold. Used to exercise cohesion-only module warnings.
const THREE_ALGO_FIXTURE: &str = r#"
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
    // File is short (below threshold) but has 3+ independent private algorithms.
    let code = THREE_ALGO_FIXTURE;
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
    let warnings = analyze_module_srp(&parsed, &config, &call_graph, &cfg_test_files, 300);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].independent_clusters, 3);
    // The warning is cohesion-driven, not length-driven: the short fixture
    // sits below the file_length threshold (ratio < 1.0).
    assert!(warnings[0].production_lines <= 300);
    assert!(warnings[0].length_score < 1.0);
}
