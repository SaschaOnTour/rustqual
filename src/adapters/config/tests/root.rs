use crate::adapters::config::*;
use std::fs;
use tempfile::TempDir;

// ── exclude_files ─────────────────────────────────────────────────

#[test]
fn test_excluded_file_compiled() {
    let mut cfg = Config {
        exclude_files: vec!["generated/**".into()],
        ..Config::default()
    };
    cfg.compile();
    assert!(cfg.is_excluded_file("generated/foo.rs"));
    assert!(!cfg.is_excluded_file("src/main.rs"));
}

// ── Config::load ──────────────────────────────────────────────────

#[test]
fn test_config_loads_rustqual_toml() {
    let tmp = TempDir::new().unwrap();
    let toml_content = r#"
exclude_files = ["generated/**"]
strict_closures = true
strict_iterator_chains = true
"#;
    fs::write(tmp.path().join("rustqual.toml"), toml_content).unwrap();

    let mut cfg = Config::load(tmp.path()).unwrap();
    assert_eq!(cfg.exclude_files, vec!["generated/**"]);
    assert!(cfg.strict_closures);
    assert!(cfg.strict_iterator_chains);
    // Globs are compiled on demand via compile()
    cfg.compile();
    assert!(cfg.compiled_exclude_files.is_some());
}

#[test]
fn test_load_missing_file() {
    let tmp = TempDir::new().unwrap();
    // No rustqual.toml in the temp directory → should fall back to defaults.
    let cfg = Config::load(tmp.path()).unwrap();
    assert!(!cfg.strict_closures);
}

#[test]
fn test_load_invalid_file_returns_error() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("rustqual.toml"),
        "this is not valid toml {{{",
    )
    .unwrap();
    let result = Config::load(tmp.path());
    assert!(result.is_err(), "Invalid TOML should produce an error");
}

#[test]
fn test_load_unknown_field_returns_error() {
    let tmp = TempDir::new().unwrap();
    let toml_content = r#"
strict_closures = false
unknown_field = true
"#;
    fs::write(tmp.path().join("rustqual.toml"), toml_content).unwrap();
    let result = Config::load(tmp.path());
    assert!(
        result.is_err(),
        "Unknown fields should produce an error with deny_unknown_fields"
    );
}

#[test]
fn test_default_values() {
    let cfg = Config::default();
    assert!(!cfg.strict_closures);
    assert!(!cfg.strict_iterator_chains);
    assert!(!cfg.allow_recursion);
    assert!(!cfg.strict_error_propagation);
}

#[test]
fn test_default_sub_configs() {
    let cfg = Config::default();
    assert!(cfg.complexity.enabled);
    assert!(cfg.duplicates.enabled);
    assert!(cfg.boilerplate.enabled);
    assert!(cfg.srp.enabled);
    assert!(cfg.coupling.enabled);
    assert!((cfg.max_suppression_ratio - DEFAULT_MAX_SUPPRESSION_RATIO).abs() < f64::EPSILON);
}

#[test]
fn test_load_with_sub_configs() {
    let tmp = TempDir::new().unwrap();
    let toml_content = r#"
[complexity]
enabled = false
max_cognitive = 20

[duplicates]
enabled = true
similarity_threshold = 0.90
"#;
    fs::write(tmp.path().join("rustqual.toml"), toml_content).unwrap();
    let cfg = Config::load(tmp.path()).unwrap();
    assert!(!cfg.complexity.enabled);
    assert_eq!(cfg.complexity.max_cognitive, 20);
    assert!(cfg.duplicates.enabled);
    assert!((cfg.duplicates.similarity_threshold - 0.90).abs() < f64::EPSILON);
}

#[test]
fn test_new_fields_default_false() {
    let tmp = TempDir::new().unwrap();
    let toml_content = r#"
strict_closures = false
"#;
    fs::write(tmp.path().join("rustqual.toml"), toml_content).unwrap();
    let cfg = Config::load(tmp.path()).unwrap();
    assert!(!cfg.allow_recursion);
    assert!(!cfg.strict_error_propagation);
}

#[test]
fn test_build_globset_empty() {
    let gs = build_globset(&[]);
    assert!(!gs.is_match("anything"));
}

#[test]
fn test_build_globset_patterns() {
    let gs = build_globset(&["test_*".into(), "main".into()]);
    assert!(gs.is_match("test_foo"));
    assert!(gs.is_match("main"));
    assert!(!gs.is_match("helper"));
}

#[test]
fn test_validate_weights_default_ok() {
    let cfg = Config::default();
    assert!(validate_weights(&cfg).is_ok());
}

#[test]
fn test_validate_weights_bad_sum() {
    let mut cfg = Config::default();
    cfg.weights.iosp = 0.50;
    let result = validate_weights(&cfg);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("must sum to 1.0"));
}

#[test]
fn test_validate_suppression_default_ok() {
    assert!(validate_suppression(&Config::default()).is_ok());
}

#[test]
fn test_validate_suppression_rejects_non_finite_and_negative() {
    // pin_headroom feeds `pin > value * (1 + headroom)`; inf/NaN disable the
    // too-loose check, a negative headroom makes it nonsensical. Reject all.
    for bad in [f64::INFINITY, f64::NAN, -0.1, -1.5] {
        let mut cfg = Config::default();
        cfg.suppression.pin_headroom = bad;
        let result = validate_suppression(&cfg);
        assert!(result.is_err(), "pin_headroom {bad} must be rejected");
        assert!(result.unwrap_err().contains("pin_headroom"));
    }
    // 0.0 (no headroom) is valid
    let mut cfg = Config::default();
    cfg.suppression.pin_headroom = 0.0;
    assert!(validate_suppression(&cfg).is_ok());
}
