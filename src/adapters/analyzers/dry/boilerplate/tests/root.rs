use crate::adapters::analyzers::dry::boilerplate::*;
use crate::config::sections::BoilerplateConfig;

fn parse(code: &str) -> Vec<(String, String, syn::File)> {
    let syntax = syn::parse_file(code).expect("parse failed");
    vec![("test.rs".to_string(), code.to_string(), syntax)]
}

// ── BP-001 ─────────────────────────────────────────────────

#[test]
fn test_bp001_trivial_from_tuple_struct() {
    let code = r#"
        struct Wrapper(String);
        impl From<String> for Wrapper {
            fn from(s: String) -> Self { Self(s) }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        findings.iter().any(|f| f.pattern_id == "BP-001"),
        "Trivial From(tuple) should be detected"
    );
}

#[test]
fn test_bp001_non_trivial_from_not_flagged() {
    let code = r#"
        struct Processed { data: Vec<u8>, len: usize }
        impl From<Vec<u8>> for Processed {
            fn from(data: Vec<u8>) -> Self {
                let len = data.len();
                Self { data, len }
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-001"),
        "Non-trivial From should not be flagged"
    );
}

// ── BP-002 ─────────────────────────────────────────────────

#[test]
fn test_bp002_trivial_display() {
    let code = r#"
        use std::fmt;
        struct Name(String);
        impl fmt::Display for Name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        findings.iter().any(|f| f.pattern_id == "BP-002"),
        "Trivial Display should be detected"
    );
}

#[test]
fn test_bp002_complex_display_not_flagged() {
    let code = r#"
        use std::fmt;
        struct Point { x: f64, y: f64 }
        impl fmt::Display for Point {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                if self.x == 0.0 {
                    write!(f, "(origin, {})", self.y)
                } else {
                    write!(f, "({}, {})", self.x, self.y)
                }
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-002"),
        "Complex Display should not be flagged"
    );
}

// ── BP-003 ─────────────────────────────────────────────────

#[test]
fn test_bp003_getter_setter_detected() {
    let code = r#"
        struct Config { a: i32, b: String, c: bool }
        impl Config {
            fn a(&self) -> &i32 { &self.a }
            fn b(&self) -> &String { &self.b }
            fn c(&self) -> &bool { &self.c }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        findings.iter().any(|f| f.pattern_id == "BP-003"),
        "3+ getters should be detected"
    );
}

#[test]
fn test_bp003_few_getters_not_flagged() {
    let code = r#"
        struct Pair { a: i32, b: i32 }
        impl Pair {
            fn a(&self) -> &i32 { &self.a }
            fn b(&self) -> &i32 { &self.b }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-003"),
        "Only 2 getters should not be flagged"
    );
}

#[test]
fn test_bp003_reports_per_getter_not_per_struct() {
    let code = r#"
        struct Config { a: i32, b: String, c: bool }
        impl Config {
            fn a(&self) -> &i32 { &self.a }
            fn b(&self) -> &String { &self.b }
            fn c(&self) -> &bool { &self.c }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    let bp003: Vec<_> = findings
        .iter()
        .filter(|f| f.pattern_id == "BP-003")
        .collect();
    assert_eq!(
        bp003.len(),
        3,
        "BP-003 should report one finding per getter, got {}",
        bp003.len()
    );
    // Each finding should be on a different line (the getter function line)
    let lines: std::collections::HashSet<usize> = bp003.iter().map(|f| f.line).collect();
    assert_eq!(lines.len(), 3, "Each BP-003 should be on a different line");
}

// ── BP-004 ─────────────────────────────────────────────────

#[test]
fn test_bp004_builder_detected() {
    let code = r#"
        struct Builder { a: i32, b: String, c: bool }
        impl Builder {
            fn with_a(mut self, v: i32) -> Self { self.a = v; self }
            fn with_b(mut self, v: String) -> Self { self.b = v; self }
            fn with_c(mut self, v: bool) -> Self { self.c = v; self }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        findings.iter().any(|f| f.pattern_id == "BP-004"),
        "3+ builder methods should be detected"
    );
}

#[test]
fn test_bp004_non_builder_not_flagged() {
    let code = r#"
        struct Thing { a: i32 }
        impl Thing {
            fn with_a(mut self, v: i32) -> Self { self.a = v; self }
            fn compute(self) -> i32 { self.a * 2 }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-004"),
        "Single builder method should not be flagged"
    );
}

// ── BP-005 ─────────────────────────────────────────────────

#[test]
fn test_bp005_manual_default_detected() {
    let code = r#"
        struct Config { count: i32, name: String, active: bool }
        impl Default for Config {
            fn default() -> Self {
                Self { count: 0, name: String::new(), active: false }
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        findings.iter().any(|f| f.pattern_id == "BP-005"),
        "Manual Default with all default values should be detected"
    );
}

#[test]
fn test_bp005_custom_default_not_flagged() {
    let code = r#"
        struct Config { count: i32, name: String }
        impl Default for Config {
            fn default() -> Self {
                Self { count: 42, name: String::new() }
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-005"),
        "Default with custom value (42) should not be flagged"
    );
}

// ── BP-006 ─────────────────────────────────────────────────

#[test]
fn test_bp006_repetitive_match_detected() {
    let code = r#"
        enum Color { Red, Blue, Green, Yellow }
        enum Shade { Red, Blue, Green, Yellow }
        fn convert(c: Color) -> Shade {
            match c {
                Color::Red => Shade::Red,
                Color::Blue => Shade::Blue,
                Color::Green => Shade::Green,
                Color::Yellow => Shade::Yellow,
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        findings.iter().any(|f| f.pattern_id == "BP-006"),
        "Repetitive enum mapping match should be detected"
    );
}

#[test]
fn test_bp006_complex_match_not_flagged() {
    let code = r#"
        enum Action { Add(i32), Remove(String), Clear }
        fn describe(a: &Action) -> &str {
            match a {
                Action::Add(_) => "adding",
                Action::Remove(_) => "removing",
                Action::Clear => "clearing",
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-006"),
        "Match with only 3 arms should not be flagged (below threshold)"
    );
}

#[test]
fn test_bp006_tuple_scrutinee_not_flagged() {
    let code = r#"
        fn dispatch(a: bool, b: bool) -> i32 {
            match (a, b) {
                (true, true) => handle_tt(),
                (true, false) => handle_tf(),
                (false, true) => handle_ft(),
                (false, false) => handle_ff(),
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-006"),
        "Match on tuple scrutinee should not be flagged"
    );
}

#[test]
fn test_bp006_or_pattern_not_flagged() {
    let code = r#"
        enum Token { A, B, C, D, E }
        fn classify(t: Token) -> &'static str {
            match t {
                Token::A | Token::B => category_ab(),
                Token::C => category_c(),
                Token::D => category_d(),
                Token::E => category_e(),
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-006"),
        "Match with or-patterns should not be flagged"
    );
}

#[test]
fn test_bp006_dispatch_bindings_not_flagged() {
    let code = r#"
        enum Msg { A(i32), B(i32), C(i32), D(i32) }
        fn dispatch(m: Msg) {
            match m {
                Msg::A(x) => handle_a(x),
                Msg::B(x) => handle_b(x),
                Msg::C(x) => handle_c(x),
                Msg::D(x) => handle_d(x),
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-006"),
        "Match with variable bindings (dispatch) should not be flagged"
    );
}

#[test]
fn test_bp006_wildcard_arm_not_flagged() {
    let code = r#"
        enum Color { Red, Blue, Green, Yellow, Other }
        fn to_shade(c: Color) -> Shade {
            match c {
                Color::Red => Shade::Red,
                Color::Blue => Shade::Blue,
                Color::Green => Shade::Green,
                _ => Shade::default(),
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-006"),
        "Match with wildcard catch-all arm should not be flagged"
    );
}

#[test]
fn test_bp006_simple_mapping_still_detected() {
    let code = r#"
        enum Color { Red, Blue, Green, Yellow }
        enum Shade { Red, Blue, Green, Yellow }
        fn convert(c: Color) -> Shade {
            match c {
                Color::Red => Shade::Red,
                Color::Blue => Shade::Blue,
                Color::Green => Shade::Green,
                Color::Yellow => Shade::Yellow,
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        findings.iter().any(|f| f.pattern_id == "BP-006"),
        "Simple unit-variant enum mapping should still be detected"
    );
}

// ── BP-007 ─────────────────────────────────────────────────

#[test]
fn test_bp007_error_enum_detected() {
    let code = r#"
        enum AppError { Io(std::io::Error), Parse(String), Net(String) }
        impl From<std::io::Error> for AppError {
            fn from(e: std::io::Error) -> Self { Self::Io(e) }
        }
        impl From<String> for AppError {
            fn from(e: String) -> Self { Self::Parse(e) }
        }
        impl From<u32> for AppError {
            fn from(e: u32) -> Self { Self::Net(e.to_string()) }
        }
    "#;
    let _findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    // Note: third From is not trivial (e.to_string()), so only 2 trivial Froms
    // which is below the threshold. Let's use a truly trivial third:
    let code2 = r#"
        enum AppError { Io(std::io::Error), Parse(String), Net(i32) }
        impl From<std::io::Error> for AppError {
            fn from(e: std::io::Error) -> Self { Self::Io(e) }
        }
        impl From<String> for AppError {
            fn from(e: String) -> Self { Self::Parse(e) }
        }
        impl From<i32> for AppError {
            fn from(e: i32) -> Self { Self::Net(e) }
        }
    "#;
    let findings2 = detect_boilerplate(&parse(code2), &BoilerplateConfig::default());
    assert!(
        findings2.iter().any(|f| f.pattern_id == "BP-007"),
        "3+ trivial From impls for same type should be detected"
    );
}

#[test]
fn test_bp007_single_from_not_flagged() {
    let code = r#"
        struct Wrapper(String);
        impl From<String> for Wrapper {
            fn from(s: String) -> Self { Self(s) }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-007"),
        "Single From impl should not trigger error enum detection"
    );
}

// ── BP-008 ─────────────────────────────────────────────────

#[test]
fn test_bp008_clone_heavy_detected() {
    let code = r#"
        struct A { x: String, y: String, z: String }
        struct B { x: String, y: String, z: String }
        impl A {
            fn to_b(&self) -> B {
                B { x: self.x.clone(), y: self.y.clone(), z: self.z.clone() }
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        findings.iter().any(|f| f.pattern_id == "BP-008"),
        "Struct construction with 3+ .clone() calls should be detected"
    );
}

#[test]
fn test_bp008_no_clones_not_flagged() {
    let code = r#"
        struct B { x: i32, y: i32, z: i32 }
        fn make_b() -> B {
            B { x: 1, y: 2, z: 3 }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-008"),
        "Struct construction without clones should not be flagged"
    );
}

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
