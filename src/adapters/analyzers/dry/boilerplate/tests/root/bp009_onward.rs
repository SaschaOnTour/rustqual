use super::*;

// ── BP-009 ─────────────────────────────────────────────────

#[test]
fn test_bp009_few_fields_not_flagged() {
    let code = r#"
        struct A { x: i32 }
        fn make_two() -> (A, A) { (A { x: 1 }, A { x: 2 }) }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-009"),
        "Structs with <3 fields should not be flagged"
    );
}

#[test]
fn test_bp009_overlapping_constructions_detected() {
    let code = r#"
        struct Config { host: String, port: u16, timeout: u64, retries: u32 }
        fn make_configs() -> (Config, Config) {
            let a = Config { host: "a".to_string(), port: 80, timeout: 30, retries: 3 };
            let b = Config { host: "b".to_string(), port: 80, timeout: 30, retries: 3 };
            (a, b)
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        findings.iter().any(|f| f.pattern_id == "BP-009"),
        "Two constructions of same type with overlapping fields should be detected"
    );
}

#[test]
fn test_bp009_different_types_not_flagged() {
    let code = r#"
        struct A { x: i32, y: i32, z: i32 }
        struct B { x: i32, y: i32, z: i32 }
        fn make() -> (A, B) {
            let a = A { x: 1, y: 2, z: 3 };
            let b = B { x: 1, y: 2, z: 3 };
            (a, b)
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-009"),
        "Different struct types should not be grouped"
    );
}

#[test]
fn test_bp009_struct_update_syntax_not_flagged() {
    let code = r#"
        struct Config { host: String, port: u16, timeout: u64, retries: u32 }
        fn make_configs(base: Config) -> Config {
            let a = Config { host: "a".to_string(), port: 80, timeout: 30, retries: 3 };
            let b = Config { host: "b".to_string(), ..base };
            b
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-009"),
        "Only one full construction (other uses ..base) should not be flagged"
    );
}

// ── BP-010 ─────────────────────────────────────────────────

#[test]
fn test_bp010_different_formats_not_flagged() {
    let code = r#"
        fn log_stuff() {
            println!("a: {}", 1);
            println!("b: {}", 2);
            println!("c: {}", 3);
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-010"),
        "Different format strings should not be flagged"
    );
}

#[test]
fn test_bp010_repeated_format_detected() {
    let code = r#"
        fn log_many() {
            println!("value: {}", 1);
            println!("value: {}", 2);
            println!("value: {}", 3);
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        findings.iter().any(|f| f.pattern_id == "BP-010"),
        "3+ identical format strings should be detected"
    );
}

#[test]
fn test_bp010_two_repetitions_not_flagged() {
    let code = r#"
        fn log_few() {
            println!("same: {}", 1);
            println!("same: {}", 2);
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-010"),
        "Only 2 repetitions should not be flagged (threshold is 3)"
    );
}

// ── Config filtering ───────────────────────────────────────

#[test]
fn test_pattern_filtering_only_selected() {
    let code = r#"
        struct W(String);
        impl From<String> for W {
            fn from(s: String) -> Self { Self(s) }
        }
        struct Config { count: i32, name: String, active: bool }
        impl Default for Config {
            fn default() -> Self {
                Self { count: 0, name: String::new(), active: false }
            }
        }
    "#;
    let config = BoilerplateConfig {
        patterns: vec!["BP-005".to_string()], // Only Default
        ..BoilerplateConfig::default()
    };
    let findings = detect_boilerplate(&parse(code), &config);
    assert!(
        findings.iter().any(|f| f.pattern_id == "BP-005"),
        "BP-005 should be detected when selected"
    );
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-001"),
        "BP-001 should be skipped when not selected"
    );
}

#[test]
fn test_suggest_crates_flag() {
    let code = r#"
        struct W(String);
        impl From<String> for W {
            fn from(s: String) -> Self { Self(s) }
        }
    "#;
    let config = BoilerplateConfig {
        suggest_crates: false,
        ..BoilerplateConfig::default()
    };
    let findings = detect_boilerplate(&parse(code), &config);
    let f = findings.iter().find(|f| f.pattern_id == "BP-001");
    assert!(f.is_some());
    assert!(
        !f.unwrap().suggestion.contains("derive_more"),
        "Should not mention crates when suggest_crates is false"
    );
}

#[test]
fn test_disabled_boilerplate_returns_empty() {
    let code = r#"
        struct W(String);
        impl From<String> for W {
            fn from(s: String) -> Self { Self(s) }
        }
    "#;
    let config = BoilerplateConfig {
        patterns: vec!["BP-999".to_string()], // No real patterns
        ..BoilerplateConfig::default()
    };
    let findings = detect_boilerplate(&parse(code), &config);
    assert!(
        findings.is_empty(),
        "No findings when no patterns are enabled"
    );
}
