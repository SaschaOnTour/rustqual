use crate::adapters::config::sections::*;

#[test]
fn test_complexity_config_defaults() {
    let c = ComplexityConfig::default();
    assert!(c.enabled);
    assert_eq!(c.max_cognitive, DEFAULT_MAX_COGNITIVE);
    assert_eq!(c.max_cyclomatic, DEFAULT_MAX_CYCLOMATIC);
    assert!(c.detect_magic_numbers);
    assert_eq!(
        c.allowed_magic_numbers,
        vec!["0", "1", "-1", "2", "0.0", "1.0"]
    );
}

#[test]
fn test_duplicates_config_defaults() {
    let c = DuplicatesConfig::default();
    assert!(c.enabled);
    assert!((c.similarity_threshold - DEFAULT_SIMILARITY_THRESHOLD).abs() < f64::EPSILON);
    assert_eq!(c.min_tokens, DEFAULT_MIN_TOKENS);
    assert_eq!(c.min_lines, DEFAULT_MIN_LINES);
    assert_eq!(c.min_statements, DEFAULT_MIN_STATEMENTS);
    assert!(c.detect_dead_code);
}

#[test]
fn test_boilerplate_config_defaults() {
    let c = BoilerplateConfig::default();
    assert!(c.enabled);
    assert!(c.patterns.is_empty());
    assert!(c.suggest_crates);
}

#[test]
fn test_srp_config_defaults() {
    let c = SrpConfig::default();
    assert!(c.enabled);
    assert!((c.smell_threshold - DEFAULT_SRP_SMELL_THRESHOLD).abs() < f64::EPSILON);
    assert_eq!(c.max_fields, DEFAULT_SRP_MAX_FIELDS);
    assert_eq!(c.max_methods, DEFAULT_SRP_MAX_METHODS);
    assert_eq!(c.file_length_baseline, DEFAULT_SRP_FILE_LENGTH_BASELINE);
    assert_eq!(c.file_length_ceiling, DEFAULT_SRP_FILE_LENGTH_CEILING);
}

#[test]
fn test_coupling_config_defaults() {
    let c = CouplingConfig::default();
    assert!(c.enabled);
    assert!((c.max_instability - DEFAULT_MAX_INSTABILITY).abs() < f64::EPSILON);
    assert_eq!(c.max_fan_in, DEFAULT_MAX_FAN_IN);
    assert_eq!(c.max_fan_out, DEFAULT_MAX_FAN_OUT_COUPLING);
}

#[test]
fn test_complexity_config_deserialize() {
    let toml_str = r#"
        enabled = false
        max_cognitive = 20
        max_cyclomatic = 15
    "#;
    let c: ComplexityConfig = toml::from_str(toml_str).unwrap();
    assert!(!c.enabled);
    assert_eq!(c.max_cognitive, 20);
    assert_eq!(c.max_cyclomatic, 15);
    // Defaults for unspecified fields
    assert!(c.detect_magic_numbers);
}

#[test]
fn test_duplicates_config_deserialize() {
    let toml_str = r#"
        enabled = true
        similarity_threshold = 0.90
        min_tokens = 50
    "#;
    let c: DuplicatesConfig = toml::from_str(toml_str).unwrap();
    assert!(c.enabled);
    assert!((c.similarity_threshold - 0.90).abs() < f64::EPSILON);
    assert_eq!(c.min_tokens, 50);
}

#[test]
fn test_srp_config_deserialize_with_weights() {
    let toml_str = r#"
        enabled = true
        weights = [0.5, 0.2, 0.1, 0.2]
    "#;
    let c: SrpConfig = toml::from_str(toml_str).unwrap();
    assert!((c.weights[0] - 0.5).abs() < f64::EPSILON);
}

// qual:allow(test) reason: "verifies production constant, no function/type call needed"
#[test]
fn test_quality_weights_sum_to_one() {
    let sum: f64 = DEFAULT_QUALITY_WEIGHTS.iter().sum();
    assert!(
        (sum - 1.0).abs() < f64::EPSILON,
        "Quality weights must sum to 1.0, got {sum}"
    );
}

#[test]
fn test_weights_config_defaults() {
    let w = WeightsConfig::default();
    assert!((w.iosp - 0.22).abs() < f64::EPSILON);
    assert!((w.complexity - 0.18).abs() < f64::EPSILON);
    assert!((w.dry - 0.13).abs() < f64::EPSILON);
    assert!((w.srp - 0.18).abs() < f64::EPSILON);
    assert!((w.coupling - 0.09).abs() < f64::EPSILON);
    assert!((w.test_quality - 0.10).abs() < f64::EPSILON);
    assert!((w.architecture - 0.10).abs() < f64::EPSILON);
}

#[test]
fn test_weights_config_as_array() {
    let w = WeightsConfig::default();
    let arr = w.as_array();
    assert_eq!(arr, DEFAULT_QUALITY_WEIGHTS);
}

#[test]
fn test_weights_config_deserialize() {
    let toml_str = r#"
        iosp = 0.25
        complexity = 0.18
        dry = 0.13
        srp = 0.15
        coupling = 0.09
        test_quality = 0.10
        architecture = 0.10
    "#;
    let w: WeightsConfig = toml::from_str(toml_str).unwrap();
    assert!((w.iosp - 0.25).abs() < f64::EPSILON);
    assert!((w.complexity - 0.18).abs() < f64::EPSILON);
    assert!((w.test_quality - 0.10).abs() < f64::EPSILON);
    assert!((w.architecture - 0.10).abs() < f64::EPSILON);
}

// qual:allow(dry) reason: "parsing test against legacy v0.5.x field is unique"
#[test]
fn test_weights_config_rejects_legacy_test_field() {
    // v1.0 Breaking Change: `test` was renamed to `test_quality`.
    // Configs with the old field name must be explicitly rejected.
    let toml_str = r#"
        iosp = 0.30
        complexity = 0.20
        dry = 0.15
        srp = 0.15
        coupling = 0.10
        test = 0.10
        architecture = 0.00
    "#;
    let result: Result<WeightsConfig, _> = toml::from_str(toml_str);
    assert!(
        result.is_err(),
        "Legacy `test` field must be rejected by deny_unknown_fields"
    );
}

#[test]
fn test_quality_weights_have_seven_dimensions() {
    // v1.0 has seven dimensions; the DEFAULT_QUALITY_WEIGHTS array must reflect that.
    assert_eq!(DEFAULT_QUALITY_WEIGHTS.len(), 7);
    let w = WeightsConfig::default();
    assert_eq!(w.as_array().len(), 7);
}

#[test]
fn test_report_config_default_is_loc_weighted() {
    let c = ReportConfig::default();
    assert_eq!(c.aggregation, "loc_weighted");
}

#[test]
fn test_report_config_parse_arithmetic() {
    let toml_str = r#"
        aggregation = "arithmetic"
    "#;
    let c: ReportConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(c.aggregation, "arithmetic");
}

#[test]
fn test_report_config_rejects_unknown_fields() {
    let toml_str = r#"
        aggregation = "loc_weighted"
        bogus = "x"
    "#;
    let result: Result<ReportConfig, _> = toml::from_str(toml_str);
    assert!(result.is_err());
}

#[test]
fn test_test_config_defaults() {
    let c = TestConfig::default();
    assert!(c.enabled);
    assert!(c.coverage_file.is_none());
}

#[test]
fn test_test_config_deserialize() {
    let toml_str = r#"
        enabled = true
        coverage_file = "lcov.info"
    "#;
    let c: TestConfig = toml::from_str(toml_str).unwrap();
    assert!(c.enabled);
    assert_eq!(c.coverage_file.as_deref(), Some("lcov.info"));
}
