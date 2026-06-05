use super::*;

#[test]
fn lcom4_constructor_and_transitive_connections() {
    // LCOM4 = number of connected components: methods sharing a field (directly
    // or transitively) merge; a constructor (returns Self) touches all fields
    // and unifies everything; fields not on the struct are ignored.
    let cases: &[Lcom4CtorCase] = &[
        (
            "transitive: a{x}, b{x,y}, c{y} connect through b",
            &[("a", &["x"]), ("b", &["x", "y"]), ("c", &["y"])],
            &[],
            &["x", "y"],
            1,
        ),
        (
            "disjoint getters without a constructor stay separate",
            &[("get_x", &["x"]), ("get_y", &["y"])],
            &[],
            &["x", "y"],
            2,
        ),
        (
            "constructor connects otherwise-disjoint getters",
            &[("get_x", &["x"]), ("get_y", &["y"])],
            &["new"],
            &["x", "y"],
            1,
        ),
        (
            "default() constructor also unifies all clusters",
            &[("get_a", &["a"]), ("get_b", &["b"]), ("get_c", &["c"])],
            &["default"],
            &["a", "b", "c"],
            1,
        ),
        (
            "field not on the struct is ignored",
            &[("a", &["x", "foreign_field"]), ("b", &["x"])],
            &[],
            &["x"],
            1,
        ),
    ];
    for (label, methods, constructors, struct_fields, expected) in cases {
        let mut owned: Vec<MethodFieldData> = methods
            .iter()
            .map(|(name, fields)| make_method(name, "T", fields, &[]))
            .collect();
        owned.extend(
            constructors
                .iter()
                .map(|name| make_constructor(name, "T", &[])),
        );
        let refs: Vec<&MethodFieldData> = owned.iter().collect();
        let fields: Vec<String> = struct_fields.iter().map(|s| s.to_string()).collect();
        let idx = build_field_method_index(&refs, &fields);
        let (lcom4, _) = compute_lcom4(&refs, &fields, &idx, &[]);
        assert_eq!(lcom4, *expected, "case {label}");
    }
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
        &[],
    );
    assert_eq!(clusters.len(), 2);
    // Each cluster should have exactly one method and one field
    for c in &clusters {
        assert_eq!(c.methods.len(), 1);
        assert_eq!(c.fields.len(), 1);
    }
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
        &[],
    );
    assert_eq!(
        lcom4, 1,
        "Methods sharing field via self.conn() call should be one component"
    );
}

// ── End-to-end collector tests exercising MethodBodyVisitor ──────

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
        300,
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
