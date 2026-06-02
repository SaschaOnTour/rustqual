use super::*;

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
        &SrpConfig::default(),
    );
    assert!(
        !warnings.is_empty(),
        "Incohesive god object should trigger SRP warning"
    );
    assert_eq!(warnings[0].struct_name, "GodObject");
}
