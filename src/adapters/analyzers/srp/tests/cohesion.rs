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

// ── End-to-end collector tests exercising MethodBodyVisitor ──────

/// Run the full SRP method collection path and return method data for a
/// single struct, so assertions can see exactly what `MethodBodyVisitor`
/// produces from real source. Bug 2 root cause was that `MethodBodyVisitor`
/// didn't descend into macro token streams, so `self.validate()` inside
/// `debug_assert!(...)` was invisible to LCOM4.
fn collect_methods_for(code: &str) -> Vec<MethodFieldData> {
    let syntax = syn::parse_file(code).expect("parse test fixture");
    let parsed = vec![("test.rs".to_string(), code.to_string(), syntax)];
    let mut result = Vec::new();
    let mut collector = crate::adapters::analyzers::srp::ImplMethodCollector {
        file: String::new(),
        methods: &mut result,
    };
    crate::adapters::analyzers::dry::visit_all_files(&parsed, &mut collector);
    result
}

#[test]
fn method_body_visitor_sees_self_calls_inside_debug_assert_macro() {
    let code = r#"
        struct Storage { buf: usize, active: bool }
        impl Storage {
            fn seq_len(&self) -> usize { self.buf }
            fn validate(&self) -> bool { self.active && self.buf > 0 }
            fn append(&mut self, n: usize) {
                self.buf = n;
                self.active = true;
                debug_assert!(self.validate());
            }
        }
    "#;
    let methods = collect_methods_for(code);
    let append = methods
        .iter()
        .find(|m| m.method_name == "append")
        .expect("append collected");
    assert!(
        append.self_method_calls.contains("validate"),
        "append should see self.validate() inside debug_assert!, got: {:?}",
        append.self_method_calls
    );
}

#[test]
fn lcom4_unites_methods_linked_via_debug_assert_macro() {
    // Bug 2 reproducer. `append` only writes `extra`, which no other
    // method touches — so field-sharing alone cannot unite the clusters.
    // The sole link is `debug_assert!(self.validate())`, which lives in
    // a macro token stream. Without visit_macro on MethodBodyVisitor,
    // LCOM4 reports 2 clusters ({readers+validate}, {append}).
    let code = r#"
        struct Store {
            a: usize,
            b: usize,
            extra: bool,
        }
        impl Store {
            fn read_a(&self) -> usize { self.a }
            fn read_b(&self) -> usize { self.b }
            fn validate(&self) -> bool { self.a > 0 && self.b > 0 }
            fn append(&mut self, flag: bool) {
                self.extra = flag;
                debug_assert!(self.validate());
            }
        }
    "#;
    let syntax = syn::parse_file(code).expect("parse fixture");
    let parsed = vec![("test.rs".to_string(), code.to_string(), syntax)];
    let analysis = crate::adapters::analyzers::srp::analyze_srp(
        &parsed,
        &SrpConfig {
            smell_threshold: 0.0,
            ..SrpConfig::default()
        },
        &HashMap::new(),
    );
    let w = analysis
        .struct_warnings
        .iter()
        .find(|w| w.struct_name == "Store")
        .expect("Store warning collected (smell_threshold=0)");
    assert_eq!(
        w.lcom4, 1,
        "Macro-linked methods should form one cluster, got {}",
        w.lcom4
    );
}

#[test]
fn lcom4_unites_methods_via_assert_eq_macro() {
    let code = r#"
        struct Pair { a: i32, b: i32 }
        impl Pair {
            fn a(&self) -> i32 { self.a }
            fn b(&self) -> i32 { self.b }
            fn check(&self) {
                assert_eq!(self.a(), self.b());
            }
        }
    "#;
    let methods = collect_methods_for(code);
    let check = methods
        .iter()
        .find(|m| m.method_name == "check")
        .expect("check collected");
    assert!(
        check.self_method_calls.contains("a") && check.self_method_calls.contains("b"),
        "check should see both self.a() and self.b() inside assert_eq!, got: {:?}",
        check.self_method_calls
    );
}

#[test]
fn lcom4_unites_methods_via_format_macro_call_edge() {
    let code = r#"
        struct View { title: String, body: String }
        impl View {
            fn title(&self) -> &str { &self.title }
            fn body(&self) -> &str { &self.body }
            fn render(&self) -> String {
                format!("{} — {}", self.title(), self.body())
            }
        }
    "#;
    let methods = collect_methods_for(code);
    let render = methods
        .iter()
        .find(|m| m.method_name == "render")
        .expect("render collected");
    assert!(
        render.self_method_calls.contains("title") && render.self_method_calls.contains("body"),
        "render should see self.title()/self.body() inside format!, got: {:?}",
        render.self_method_calls
    );
}
