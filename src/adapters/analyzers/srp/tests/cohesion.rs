use crate::adapters::analyzers::srp::cohesion::*;
use crate::adapters::analyzers::srp::{MethodFieldData, StructInfo};
use crate::config::sections::SrpConfig;
use std::collections::{HashMap, HashSet};

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
