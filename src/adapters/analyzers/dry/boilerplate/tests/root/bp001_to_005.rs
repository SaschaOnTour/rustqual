use super::*;

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
