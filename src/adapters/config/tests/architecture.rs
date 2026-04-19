use crate::adapters::config::architecture::*;

#[test]
fn test_architecture_config_default_is_disabled() {
    let c = ArchitectureConfig::default();
    assert!(!c.enabled);
    assert!(c.layers.order.is_empty());
    assert_eq!(c.layers.unmatched_behavior, "composition_root");
    assert!(c.external_crates.is_empty());
    assert!(c.forbidden_rules.is_empty());
    assert!(c.patterns.is_empty());
    assert!(c.trait_contracts.is_empty());
}

#[test]
fn test_reexport_points_default_is_lib_and_main() {
    let c = ReexportPointsConfig::default();
    assert_eq!(c.paths, vec!["src/lib.rs", "src/main.rs"]);
}

#[test]
fn test_architecture_enabled_minimal() {
    let toml_str = r#"
        enabled = true
    "#;
    let c: ArchitectureConfig = toml::from_str(toml_str).unwrap();
    assert!(c.enabled);
}

#[test]
fn test_architecture_layers_parse() {
    let toml_str = r#"
        [layers]
        order = ["domain", "port", "application", "adapter"]
        unmatched_behavior = "composition_root"

        [layers.domain]
        paths = ["src/domain/**"]

        [layers.application]
        paths = ["src/app/**"]
    "#;
    let c: ArchitectureConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(
        c.layers.order,
        vec!["domain", "port", "application", "adapter"]
    );
    assert_eq!(c.layers.unmatched_behavior, "composition_root");
    assert_eq!(c.layers.definitions.len(), 2);
    assert_eq!(c.layers.definitions["domain"].paths, vec!["src/domain/**"]);
    assert_eq!(
        c.layers.definitions["application"].paths,
        vec!["src/app/**"]
    );
}

#[test]
fn test_architecture_external_crates_parse() {
    let toml_str = r#"
        [external_crates]
        "pv_core" = "domain"
        "pv_port_*" = "port"
        "pv_adp_*" = "adapter"
    "#;
    let c: ArchitectureConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(c.external_crates.len(), 3);
    assert_eq!(c.external_crates["pv_core"], "domain");
    assert_eq!(c.external_crates["pv_port_*"], "port");
}

#[test]
fn test_architecture_forbidden_parse() {
    let toml_str = r#"
        [[forbidden]]
        from = "src/adapters/a/**"
        to = "src/adapters/b/**"
        reason = "peers are isolated"

        [[forbidden]]
        from = "src/domain/**"
        to = "**"
        except = ["src/domain/**", "src/shared/**"]
        reason = "domain is framework-free"
    "#;
    let c: ArchitectureConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(c.forbidden_rules.len(), 2);
    let first = &c.forbidden_rules[0];
    assert_eq!(first.from, "src/adapters/a/**");
    assert_eq!(first.to, "src/adapters/b/**");
    assert!(first.except.is_empty());
    assert_eq!(first.reason, "peers are isolated");
    let second = &c.forbidden_rules[1];
    assert_eq!(second.except.len(), 2);
}

#[test]
fn test_symbol_pattern_all_matchers_parse() {
    let toml_str = r#"
        [[pattern]]
        name = "everything"
        forbidden_in = ["src/**"]
        forbid_path_prefix = ["tokio::"]
        forbid_method_call = ["unwrap"]
        forbid_function_call = ["Box::new"]
        forbid_macro_call = ["println"]
        forbid_item_kind = ["unsafe_fn"]
        forbid_derive = ["Serialize"]
        forbid_glob_import = true
        regex = 'some\s+pattern'
        reason = "kitchen sink"
    "#;
    let c: ArchitectureConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(c.patterns.len(), 1);
    let p = &c.patterns[0];
    assert_eq!(p.name, "everything");
    assert_eq!(
        p.forbidden_in.as_ref().unwrap(),
        &vec!["src/**".to_string()]
    );
    assert!(p.allowed_in.is_none());
    assert_eq!(
        p.forbid_path_prefix.as_ref().unwrap(),
        &vec!["tokio::".to_string()]
    );
    assert_eq!(
        p.forbid_method_call.as_ref().unwrap(),
        &vec!["unwrap".to_string()]
    );
    assert_eq!(
        p.forbid_function_call.as_ref().unwrap(),
        &vec!["Box::new".to_string()]
    );
    assert_eq!(
        p.forbid_macro_call.as_ref().unwrap(),
        &vec!["println".to_string()]
    );
    assert_eq!(
        p.forbid_item_kind.as_ref().unwrap(),
        &vec!["unsafe_fn".to_string()]
    );
    assert_eq!(
        p.forbid_derive.as_ref().unwrap(),
        &vec!["Serialize".to_string()]
    );
    assert_eq!(p.forbid_glob_import, Some(true));
    assert_eq!(p.regex.as_deref(), Some(r"some\s+pattern"));
}

#[test]
fn test_symbol_pattern_allowed_in_alternative() {
    let toml_str = r#"
        [[pattern]]
        name = "anyhow_only_at_boundary"
        allowed_in = ["src/main.rs", "tests/**"]
        forbid_path_prefix = ["anyhow::"]
        reason = "typed errors outside main"
    "#;
    let c: ArchitectureConfig = toml::from_str(toml_str).unwrap();
    let p = &c.patterns[0];
    assert!(p.forbidden_in.is_none());
    assert_eq!(p.allowed_in.as_ref().unwrap().len(), 2);
}

#[test]
fn test_trait_contract_all_checks_parse() {
    let toml_str = r#"
        [[trait_contract]]
        name = "port_traits"
        scope = "src/ports/**"
        receiver_may_be = ["shared_ref"]
        required_param_type_contains = "CancellationToken"
        forbidden_return_type_contains = ["anyhow::", "Box<dyn"]
        forbidden_error_variant_contains = ["rusqlite::"]
        error_types = ["StoreError"]
        methods_must_be_async = true
        must_be_object_safe = true
        required_supertraits_contain = ["Send", "Sync"]
    "#;
    let c: ArchitectureConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(c.trait_contracts.len(), 1);
    let tc = &c.trait_contracts[0];
    assert_eq!(tc.name, "port_traits");
    assert_eq!(tc.scope, "src/ports/**");
    assert_eq!(
        tc.receiver_may_be.as_ref().unwrap(),
        &vec!["shared_ref".to_string()]
    );
    assert_eq!(
        tc.required_param_type_contains.as_deref(),
        Some("CancellationToken")
    );
    assert_eq!(tc.forbidden_return_type_contains.as_ref().unwrap().len(), 2);
    assert_eq!(
        tc.error_types.as_ref().unwrap(),
        &vec!["StoreError".to_string()]
    );
    assert_eq!(tc.methods_must_be_async, Some(true));
    assert_eq!(tc.must_be_object_safe, Some(true));
    assert_eq!(tc.required_supertraits_contain.as_ref().unwrap().len(), 2);
}

#[test]
fn test_unknown_field_rejected() {
    // deny_unknown_fields on ArchitectureConfig
    let toml_str = r#"
        enabled = true
        unexpected_field = "oops"
    "#;
    let result: Result<ArchitectureConfig, _> = toml::from_str(toml_str);
    assert!(result.is_err(), "unknown top-level fields must be rejected");
}

#[test]
fn test_symbol_pattern_unknown_field_rejected() {
    let toml_str = r#"
        [[pattern]]
        name = "x"
        forbidden_in = ["src/**"]
        reason = "y"
        bogus_matcher = ["z"]
    "#;
    let result: Result<ArchitectureConfig, _> = toml::from_str(toml_str);
    assert!(
        result.is_err(),
        "unknown fields in pattern must be rejected"
    );
}

#[test]
fn test_forbidden_unknown_field_rejected() {
    let toml_str = r#"
        [[forbidden]]
        from = "a"
        to = "b"
        reason = "c"
        bogus = "d"
    "#;
    let result: Result<ArchitectureConfig, _> = toml::from_str(toml_str);
    assert!(result.is_err());
}

#[test]
fn test_reexport_points_custom_paths() {
    let toml_str = r#"
        [reexport_points]
        paths = ["src/lib.rs", "src/main.rs", "src/prelude.rs"]
    "#;
    let c: ArchitectureConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(c.reexport_points.paths.len(), 3);
    assert!(c
        .reexport_points
        .paths
        .iter()
        .any(|p| p == "src/prelude.rs"));
}
