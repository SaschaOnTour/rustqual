use std::collections::{HashMap, HashSet};

use crate::config::sections::SrpConfig;

use super::union_find::UnionFind;
use super::{MethodFieldData, ResponsibilityCluster, SrpWarning, StructInfo};

/// Build SRP warnings for structs that exceed the smell threshold.
/// Operation: groups methods by parent type, computes LCOM4 and composite
/// score per struct via closures (filter_map), no own calls.
pub fn build_struct_warnings(
    structs: &[StructInfo],
    methods: &[MethodFieldData],
    config: &SrpConfig,
) -> Vec<SrpWarning> {
    // Group methods by parent type
    let mut methods_by_type: HashMap<&str, Vec<&MethodFieldData>> = HashMap::new();
    for m in methods {
        methods_by_type.entry(&m.parent_type).or_default().push(m);
    }

    structs
        .iter()
        .filter_map(|s| {
            let type_methods = methods_by_type.get(s.name.as_str());
            let method_list: Vec<&MethodFieldData> =
                type_methods.map(|v| v.to_vec()).unwrap_or_default();

            // Skip structs with fewer than 2 instance methods (LCOM4 undefined)
            if method_list.len() < 2 {
                return None;
            }

            let field_idx = build_field_method_index(&method_list, &s.fields);
            let (lcom4, clusters) = compute_lcom4(&method_list, &s.fields, &field_idx);
            let fan_out = compute_fan_out(&method_list);
            let composite =
                compute_composite_score(lcom4, s.fields.len(), method_list.len(), fan_out, config);

            if composite >= config.smell_threshold {
                Some(SrpWarning {
                    struct_name: s.name.clone(),
                    file: s.file.clone(),
                    line: s.line,
                    lcom4,
                    field_count: s.fields.len(),
                    method_count: method_list.len(),
                    fan_out,
                    composite_score: composite,
                    clusters,
                    suppressed: false,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Build the field-to-methods index, resolving self-method calls one level deep.
/// Operation: iterates methods, expands field accesses from self-call targets.
fn build_field_method_index<'a>(
    methods: &[&'a MethodFieldData],
    struct_fields: &'a [String],
) -> HashMap<&'a str, Vec<usize>> {
    let direct_fields: HashMap<&str, &HashSet<String>> = methods
        .iter()
        .map(|m| (m.method_name.as_str(), &m.field_accesses))
        .collect();

    let mut field_to_methods: HashMap<&str, Vec<usize>> = HashMap::new();
    methods.iter().enumerate().for_each(|(i, m)| {
        let mut fields_to_add: Vec<&str> = if m.is_constructor {
            struct_fields.iter().map(|f| f.as_str()).collect()
        } else {
            m.field_accesses
                .iter()
                .filter(|f| struct_fields.iter().any(|sf| sf == *f))
                .map(|f| f.as_str())
                .collect()
        };
        // Resolve self-method calls: add callee's direct field accesses
        m.self_method_calls.iter().for_each(|callee| {
            if let Some(callee_fields) = direct_fields.get(callee.as_str()) {
                callee_fields
                    .iter()
                    .filter(|f| struct_fields.iter().any(|sf| sf == *f))
                    .for_each(|f| fields_to_add.push(f.as_str()));
            }
        });
        fields_to_add.iter().for_each(|&field| {
            field_to_methods.entry(field).or_default().push(i);
        });
    });
    field_to_methods
}

/// Compute LCOM4: number of connected components in the method-field graph.
/// Operation: Union-Find on method indices connected by shared field accesses.
/// Uses closures to wrap UnionFind calls for IOSP lenient-mode compliance.
fn compute_lcom4(
    methods: &[&MethodFieldData],
    struct_fields: &[String],
    field_to_methods: &HashMap<&str, Vec<usize>>,
) -> (usize, Vec<ResponsibilityCluster>) {
    let n = methods.len();
    if n == 0 {
        return (0, vec![]);
    }

    let make_uf = |size| UnionFind::new(size);
    let mut uf = make_uf(n);
    let unite = |uf: &mut UnionFind, a, b| uf.union(a, b);
    let components = |uf: &mut UnionFind| uf.component_members();
    // Union methods that share fields
    field_to_methods.values().for_each(|indices| {
        indices.windows(2).for_each(|w| unite(&mut uf, w[0], w[1]));
    });
    // Build clusters from connected components
    let component_members = components(&mut uf);
    let clusters: Vec<ResponsibilityCluster> = component_members
        .values()
        .map(|member_indices| {
            let cluster_methods: Vec<String> = member_indices
                .iter()
                .map(|&i| methods[i].method_name.clone())
                .collect();
            let cluster_fields: HashSet<String> = member_indices
                .iter()
                .flat_map(|&i| {
                    methods[i]
                        .field_accesses
                        .iter()
                        .filter(|f| struct_fields.iter().any(|sf| sf == *f))
                        .cloned()
                })
                .collect();
            ResponsibilityCluster {
                methods: cluster_methods,
                fields: cluster_fields.into_iter().collect(),
            }
        })
        .collect();
    (component_members.len(), clusters)
}

/// Compute total fan-out: distinct external call targets across all methods.
/// Operation: set union.
fn compute_fan_out(methods: &[&MethodFieldData]) -> usize {
    let all_targets: HashSet<&str> = methods
        .iter()
        .flat_map(|m| m.call_targets.iter().map(|s| s.as_str()))
        .collect();
    all_targets.len()
}

/// Compute the composite SRP smell score from sub-metrics.
/// Operation: arithmetic normalization + weighted sum.
fn compute_composite_score(
    lcom4: usize,
    field_count: usize,
    method_count: usize,
    fan_out: usize,
    config: &SrpConfig,
) -> f64 {
    // Normalize LCOM4: 0 when <=1 (cohesive), scales linearly above threshold
    let lcom4_norm = if lcom4 <= 1 {
        0.0
    } else {
        let excess = (lcom4 - 1) as f64;
        let threshold_range = (config.lcom4_threshold.max(1) - 1) as f64;
        if threshold_range > 0.0 {
            (excess / threshold_range).min(1.0)
        } else {
            1.0
        }
    };

    let field_norm = (field_count as f64 / config.max_fields as f64).min(1.0);
    let method_norm = (method_count as f64 / config.max_methods as f64).min(1.0);
    let fan_out_norm = (fan_out as f64 / config.max_fan_out as f64).min(1.0);

    let [w_lcom4, w_fields, w_methods, w_fan_out] = config.weights;

    w_lcom4 * lcom4_norm
        + w_fields * field_norm
        + w_methods * method_norm
        + w_fan_out * fan_out_norm
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::sections::SrpConfig;

    fn make_method(name: &str, parent: &str, fields: &[&str], calls: &[&str]) -> MethodFieldData {
        MethodFieldData {
            method_name: name.to_string(),
            parent_type: parent.to_string(),
            field_accesses: fields.iter().map(|s| s.to_string()).collect(),
            call_targets: calls.iter().map(|s| s.to_string()).collect(),
            self_method_calls: HashSet::new(),
            is_constructor: false,
        }
    }

    fn make_method_with_self_calls(
        name: &str,
        parent: &str,
        fields: &[&str],
        self_calls: &[&str],
    ) -> MethodFieldData {
        MethodFieldData {
            method_name: name.to_string(),
            parent_type: parent.to_string(),
            field_accesses: fields.iter().map(|s| s.to_string()).collect(),
            call_targets: HashSet::new(),
            self_method_calls: self_calls.iter().map(|s| s.to_string()).collect(),
            is_constructor: false,
        }
    }

    fn make_constructor(name: &str, parent: &str, calls: &[&str]) -> MethodFieldData {
        MethodFieldData {
            method_name: name.to_string(),
            parent_type: parent.to_string(),
            field_accesses: HashSet::new(),
            call_targets: calls.iter().map(|s| s.to_string()).collect(),
            self_method_calls: HashSet::new(),
            is_constructor: true,
        }
    }

    fn make_struct(name: &str, fields: &[&str]) -> StructInfo {
        StructInfo {
            name: name.to_string(),
            file: "test.rs".to_string(),
            line: 1,
            fields: fields.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn test_lcom4_fully_cohesive() {
        // All methods access the same field → LCOM4 = 1
        let m1 = make_method("a", "Foo", &["x"], &[]);
        let m2 = make_method("b", "Foo", &["x"], &[]);
        let m3 = make_method("c", "Foo", &["x"], &[]);
        let methods: Vec<&MethodFieldData> = vec![&m1, &m2, &m3];
        let fields = vec!["x".to_string(), "y".to_string()];
        let (lcom4, clusters) = compute_lcom4(
            &methods,
            &fields,
            &build_field_method_index(&methods, &fields),
        );
        assert_eq!(lcom4, 1);
        assert_eq!(clusters.len(), 1);
    }

    #[test]
    fn test_lcom4_two_clusters() {
        // Two disjoint groups of methods
        let m1 = make_method("a", "Foo", &["x"], &[]);
        let m2 = make_method("b", "Foo", &["x"], &[]);
        let m3 = make_method("c", "Foo", &["y"], &[]);
        let m4 = make_method("d", "Foo", &["y"], &[]);
        let methods: Vec<&MethodFieldData> = vec![&m1, &m2, &m3, &m4];
        let fields = vec!["x".to_string(), "y".to_string()];
        let (lcom4, clusters) = compute_lcom4(
            &methods,
            &fields,
            &build_field_method_index(&methods, &fields),
        );
        assert_eq!(lcom4, 2);
        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn test_lcom4_no_shared_fields() {
        // Each method accesses a unique field → N components
        let m1 = make_method("a", "Foo", &["x"], &[]);
        let m2 = make_method("b", "Foo", &["y"], &[]);
        let m3 = make_method("c", "Foo", &["z"], &[]);
        let methods: Vec<&MethodFieldData> = vec![&m1, &m2, &m3];
        let fields = vec!["x".to_string(), "y".to_string(), "z".to_string()];
        let (lcom4, _) = compute_lcom4(
            &methods,
            &fields,
            &build_field_method_index(&methods, &fields),
        );
        assert_eq!(lcom4, 3);
    }

    #[test]
    fn test_lcom4_empty_methods() {
        let methods: Vec<&MethodFieldData> = vec![];
        let fields = vec!["x".to_string()];
        let (lcom4, clusters) = compute_lcom4(
            &methods,
            &fields,
            &build_field_method_index(&methods, &fields),
        );
        assert_eq!(lcom4, 0);
        assert!(clusters.is_empty());
    }

    #[test]
    fn test_lcom4_method_with_no_field_access() {
        // Method that doesn't access any struct fields → isolated component
        let m1 = make_method("a", "Foo", &["x"], &[]);
        let m2 = make_method("b", "Foo", &[], &["helper"]);
        let methods: Vec<&MethodFieldData> = vec![&m1, &m2];
        let fields = vec!["x".to_string()];
        let (lcom4, _) = compute_lcom4(
            &methods,
            &fields,
            &build_field_method_index(&methods, &fields),
        );
        assert_eq!(lcom4, 2);
    }

    #[test]
    fn test_fan_out_distinct_targets() {
        let m1 = make_method("a", "Foo", &[], &["helper", "process"]);
        let m2 = make_method("b", "Foo", &[], &["helper", "format"]);
        let methods: Vec<&MethodFieldData> = vec![&m1, &m2];
        let fan_out = compute_fan_out(&methods);
        assert_eq!(fan_out, 3); // helper, process, format
    }

    #[test]
    fn test_fan_out_empty() {
        let m1 = make_method("a", "Foo", &["x"], &[]);
        let methods: Vec<&MethodFieldData> = vec![&m1];
        let fan_out = compute_fan_out(&methods);
        assert_eq!(fan_out, 0);
    }

    #[test]
    fn test_composite_score_fully_cohesive() {
        let config = SrpConfig::default();
        // LCOM4=1, small struct → low score
        let score = compute_composite_score(1, 3, 3, 0, &config);
        assert!(
            score < config.smell_threshold,
            "Cohesive struct should score below threshold, got {score}"
        );
    }

    #[test]
    fn test_composite_score_high_lcom4() {
        let config = SrpConfig::default();
        // LCOM4=4, many fields, many methods, high fan-out
        let score = compute_composite_score(4, 15, 20, 12, &config);
        assert!(
            score >= config.smell_threshold,
            "Incohesive struct should exceed threshold, got {score}"
        );
    }

    #[test]
    fn test_composite_score_lcom4_one_is_zero() {
        let config = SrpConfig::default();
        let score_cohesive = compute_composite_score(1, 5, 5, 2, &config);
        let score_incohesive = compute_composite_score(3, 5, 5, 2, &config);
        assert!(score_incohesive > score_cohesive);
    }

    #[test]
    fn test_build_struct_warnings_no_warning_for_small_struct() {
        let structs = vec![make_struct("Counter", &["count"])];
        let m1 = make_method("increment", "Counter", &["count"], &[]);
        let m2 = make_method("get", "Counter", &["count"], &[]);
        let methods = vec![m1, m2];
        let config = SrpConfig::default();
        let warnings = build_struct_warnings(&structs, &methods, &config);
        assert!(warnings.is_empty(), "Small cohesive struct should not warn");
    }

    #[test]
    fn test_build_struct_warnings_single_method_skipped() {
        // Structs with <2 methods are skipped (LCOM4 is undefined)
        let structs = vec![make_struct("Solo", &["x", "y", "z"])];
        let m1 = make_method("do_it", "Solo", &["x"], &[]);
        let methods = vec![m1];
        let config = SrpConfig::default();
        let warnings = build_struct_warnings(&structs, &methods, &config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_build_struct_warnings_no_methods_skipped() {
        let structs = vec![make_struct("Data", &["x", "y"])];
        let methods = vec![];
        let config = SrpConfig::default();
        let warnings = build_struct_warnings(&structs, &methods, &config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_build_struct_warnings_triggers_for_incohesive() {
        // Create a struct with clearly disjoint method groups + high fan-out
        let structs = vec![make_struct(
            "GodObject",
            &[
                "db", "cache", "logger", "metrics", "config", "state", "buffer", "queue", "pool",
                "handler", "router", "auth",
            ],
        )];
        let methods = vec![
            make_method(
                "read_db",
                "GodObject",
                &["db"],
                &["query", "parse", "validate"],
            ),
            make_method("write_db", "GodObject", &["db"], &["insert", "commit"]),
            make_method(
                "read_cache",
                "GodObject",
                &["cache"],
                &["get_key", "deserialize"],
            ),
            make_method(
                "write_cache",
                "GodObject",
                &["cache"],
                &["set_key", "serialize"],
            ),
            make_method("log_info", "GodObject", &["logger"], &["format_log"]),
            make_method(
                "log_error",
                "GodObject",
                &["logger", "metrics"],
                &["format_log", "increment"],
            ),
            make_method(
                "route_request",
                "GodObject",
                &["router", "handler"],
                &["match_path", "dispatch"],
            ),
            make_method(
                "authenticate",
                "GodObject",
                &["auth", "config"],
                &["verify_token", "check_role"],
            ),
            make_method(
                "flush_buffer",
                "GodObject",
                &["buffer", "queue"],
                &["drain", "send"],
            ),
            make_method(
                "manage_pool",
                "GodObject",
                &["pool", "state"],
                &["allocate", "release"],
            ),
        ];
        let config = SrpConfig::default();
        let warnings = build_struct_warnings(&structs, &methods, &config);
        assert!(
            !warnings.is_empty(),
            "Incohesive god object should trigger SRP warning"
        );
        assert_eq!(warnings[0].struct_name, "GodObject");
    }

    #[test]
    fn test_lcom4_transitive_connection() {
        // a→{x}, b→{x,y}, c→{y} → all connected through b
        let m1 = make_method("a", "Foo", &["x"], &[]);
        let m2 = make_method("b", "Foo", &["x", "y"], &[]);
        let m3 = make_method("c", "Foo", &["y"], &[]);
        let methods: Vec<&MethodFieldData> = vec![&m1, &m2, &m3];
        let fields = vec!["x".to_string(), "y".to_string()];
        let (lcom4, _) = compute_lcom4(
            &methods,
            &fields,
            &build_field_method_index(&methods, &fields),
        );
        assert_eq!(
            lcom4, 1,
            "Transitively connected methods should form one component"
        );
    }

    #[test]
    fn test_cluster_contains_correct_fields() {
        let m1 = make_method("a", "Foo", &["x"], &[]);
        let m2 = make_method("b", "Foo", &["y"], &[]);
        let methods: Vec<&MethodFieldData> = vec![&m1, &m2];
        let fields = vec!["x".to_string(), "y".to_string()];
        let (_, clusters) = compute_lcom4(
            &methods,
            &fields,
            &build_field_method_index(&methods, &fields),
        );
        assert_eq!(clusters.len(), 2);
        // Each cluster should have exactly one method and one field
        for c in &clusters {
            assert_eq!(c.methods.len(), 1);
            assert_eq!(c.fields.len(), 1);
        }
    }

    #[test]
    fn test_lcom4_constructor_connects_all_fields() {
        // Constructor (returns Self) should connect all methods via shared fields
        let m1 = make_method("get_x", "Foo", &["x"], &[]);
        let m2 = make_method("get_y", "Foo", &["y"], &[]);
        let m3 = make_constructor("new", "Foo", &[]);
        let methods: Vec<&MethodFieldData> = vec![&m1, &m2, &m3];
        let fields = vec!["x".to_string(), "y".to_string()];
        let (lcom4, _) = compute_lcom4(
            &methods,
            &fields,
            &build_field_method_index(&methods, &fields),
        );
        // Constructor touches all fields → connects get_x and get_y → LCOM4 = 1
        assert_eq!(
            lcom4, 1,
            "Constructor should connect disjoint method groups"
        );
    }

    #[test]
    fn test_lcom4_without_constructor_stays_disjoint() {
        // Without constructor, disjoint getters stay separate
        let m1 = make_method("get_x", "Foo", &["x"], &[]);
        let m2 = make_method("get_y", "Foo", &["y"], &[]);
        let methods: Vec<&MethodFieldData> = vec![&m1, &m2];
        let fields = vec!["x".to_string(), "y".to_string()];
        let (lcom4, _) = compute_lcom4(
            &methods,
            &fields,
            &build_field_method_index(&methods, &fields),
        );
        assert_eq!(lcom4, 2, "Without constructor, disjoint groups remain");
    }

    #[test]
    fn test_lcom4_constructor_default_pattern() {
        // fn default() -> Self is also a constructor
        let m1 = make_method("get_a", "Config", &["a"], &[]);
        let m2 = make_method("get_b", "Config", &["b"], &[]);
        let m3 = make_method("get_c", "Config", &["c"], &[]);
        let m4 = make_constructor("default", "Config", &[]);
        let methods: Vec<&MethodFieldData> = vec![&m1, &m2, &m3, &m4];
        let fields = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let (lcom4, _) = compute_lcom4(
            &methods,
            &fields,
            &build_field_method_index(&methods, &fields),
        );
        assert_eq!(lcom4, 1, "Default constructor should unify all clusters");
    }

    #[test]
    fn test_lcom4_ignores_non_struct_fields() {
        // Method accesses a field that's not part of the struct → should be ignored
        let m1 = make_method("a", "Foo", &["x", "foreign_field"], &[]);
        let m2 = make_method("b", "Foo", &["x"], &[]);
        let methods: Vec<&MethodFieldData> = vec![&m1, &m2];
        let fields = vec!["x".to_string()]; // foreign_field is not a struct field
        let (lcom4, _) = compute_lcom4(
            &methods,
            &fields,
            &build_field_method_index(&methods, &fields),
        );
        assert_eq!(lcom4, 1, "Both methods share 'x', foreign field ignored");
    }

    #[test]
    fn test_lcom4_self_method_call_resolves_field_access() {
        // conn() accesses self.conn directly
        // query() calls self.conn() — should transitively share 'conn' field
        // insert() calls self.conn() — should also share 'conn' field
        // Without resolution: query and insert have no direct field access → LCOM4=3
        // With resolution: all three share 'conn' → LCOM4=1
        let conn = make_method("conn", "Database", &["conn"], &[]);
        let query = make_method_with_self_calls("query", "Database", &[], &["conn"]);
        let insert = make_method_with_self_calls("insert", "Database", &[], &["conn"]);
        let methods: Vec<&MethodFieldData> = vec![&conn, &query, &insert];
        let fields = vec!["conn".to_string()];
        let (lcom4, _) = compute_lcom4(
            &methods,
            &fields,
            &build_field_method_index(&methods, &fields),
        );
        assert_eq!(
            lcom4, 1,
            "Methods sharing field via self.conn() call should be one component"
        );
    }
}
