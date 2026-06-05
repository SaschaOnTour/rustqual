use super::*;

#[test]
fn test_build_struct_warnings_no_warning_for_small_struct() {
    let structs = vec![make_struct("Counter", &["count"])];
    let m1 = make_method("increment", "Counter", &["count"], &[]);
    let m2 = make_method("get", "Counter", &["count"], &[]);
    let methods = vec![m1, m2];
    let config = SrpConfig::default();
    let warnings = build_struct_warnings(&structs, &methods, &[], &config);
    assert!(warnings.is_empty(), "Small cohesive struct should not warn");
}

/// A non-mechanical trait method (e.g. a `Visit` override) that calls two
/// disjoint-field helpers BRIDGES them: with the bridge the helpers cohere
/// (LCOM4=1, no warning); without it they fragment and the struct trips SRP.
/// This is the visitor case — the `visit_*` methods tie the helpers together.
#[test]
fn test_bridge_unions_disjoint_helpers() {
    // 12 fields so field_norm maxes out; two helpers touch disjoint fields.
    let field_names: Vec<String> = ('a'..='l').map(|c| c.to_string()).collect();
    let fields: Vec<&str> = field_names.iter().map(String::as_str).collect();
    let structs = vec![make_struct("Visitor", &fields)];
    let skip = make_method("skip", "Visitor", &["a"], &[]);
    let push = make_method("push", "Visitor", &["b"], &[]);
    let methods = vec![skip, push];
    let config = SrpConfig::default();

    // Without a bridge: the two helpers are disconnected → god-struct.
    let no_bridge = build_struct_warnings(&structs, &methods, &[], &config);
    assert!(
        !no_bridge.is_empty(),
        "disjoint helpers with no bridge should trip SRP"
    );

    // With a `visit_*` bridge calling both helpers: they cohere → no warning.
    let bridge = make_method_with_self_calls("visit_expr", "Visitor", &[], &["skip", "push"]);
    let bridged = build_struct_warnings(&structs, &methods, &[bridge], &config);
    assert!(
        bridged.is_empty(),
        "a trait-method bridge tying the helpers together clears the false god-struct"
    );
}

#[test]
fn test_build_struct_warnings_single_method_skipped() {
    // Structs with <2 methods are skipped (LCOM4 is undefined)
    let structs = vec![make_struct("Solo", &["x", "y", "z"])];
    let m1 = make_method("do_it", "Solo", &["x"], &[]);
    let methods = vec![m1];
    let config = SrpConfig::default();
    let warnings = build_struct_warnings(&structs, &methods, &[], &config);
    assert!(warnings.is_empty());
}

#[test]
fn test_build_struct_warnings_no_methods_skipped() {
    let structs = vec![make_struct("Data", &["x", "y"])];
    let methods = vec![];
    let config = SrpConfig::default();
    let warnings = build_struct_warnings(&structs, &methods, &[], &config);
    assert!(warnings.is_empty());
}

/// A 12-field "god object" with disjoint field groups and high fan-out — the
/// incohesive struct fixture that must trip the struct-SRP warning.
fn god_object_struct() -> Vec<StructInfo> {
    vec![make_struct(
        "GodObject",
        &[
            "db", "cache", "logger", "metrics", "config", "state", "buffer", "queue", "pool",
            "handler", "router", "auth",
        ],
    )]
}

/// Ten methods in clearly disjoint responsibility clusters (db / cache /
/// logging / routing / auth / buffering / pooling) — the incohesive arrange.
fn god_object_methods() -> Vec<MethodFieldData> {
    vec![
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
    ]
}

#[test]
fn test_build_struct_warnings_triggers_for_incohesive() {
    let warnings = build_struct_warnings(
        &god_object_struct(),
        &god_object_methods(),
        &[],
        &SrpConfig::default(),
    );
    assert!(
        !warnings.is_empty(),
        "Incohesive god object should trigger SRP warning"
    );
    let w = &warnings[0];
    assert_eq!(w.struct_name, "GodObject");
    // The spec's core SRP-001 contract is the NAMED disjoint clusters, not just
    // "this struct warned". The 7 field-groups (db / cache / logger+metrics /
    // router+handler / auth+config / buffer+queue / pool+state) are disjoint, so
    // the warning must surface lcom4=7 AND name all 7 clusters with the right
    // members — otherwise the "split into these types" refactor isn't mechanical.
    assert_eq!(w.lcom4, 7, "seven disjoint field-groups → 7 clusters");
    assert_eq!(
        w.clusters.len(),
        w.lcom4,
        "every cluster is named in the warning"
    );
    let db = w
        .clusters
        .iter()
        .find(|c| c.fields.contains(&"db".to_string()))
        .expect("a db responsibility cluster is named");
    let mut db_methods = db.methods.clone();
    db_methods.sort();
    assert_eq!(
        db_methods,
        vec!["read_db", "write_db"],
        "the db cluster groups exactly its own methods"
    );
}

#[test]
fn test_build_struct_warnings_two_methods_analysed_not_skipped() {
    // Exactly 2 methods is the boundary where LCOM4 becomes defined, so the
    // skip guard must be `len < 2` — `== 2` or `<= 2` would wrongly drop it.
    // `smell_threshold = 0.0` makes any analysed struct warn, isolating the
    // skip decision from the score.
    let structs = vec![make_struct("Pair", &["a", "b"])];
    let methods = vec![
        make_method("use_a", "Pair", &["a"], &[]),
        make_method("use_b", "Pair", &["b"], &[]),
    ];
    let config = SrpConfig {
        smell_threshold: 0.0,
        ..SrpConfig::default()
    };
    let warnings = build_struct_warnings(&structs, &methods, &[], &config);
    assert_eq!(
        warnings.len(),
        1,
        "a 2-method struct must be analysed, not skipped"
    );
    assert_eq!(warnings[0].struct_name, "Pair");
}
