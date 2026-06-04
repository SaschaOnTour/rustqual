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

#[test]
fn test_bp001_struct_literal_from_detected() {
    // A `From` returning a struct literal with path fields is trivial; guards the
    // `Expr::Struct` arm of the triviality check.
    let code =
        "struct W { a: i32 } impl From<i32> for W { fn from(a: i32) -> Self { Self { a } } }";
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        findings.iter().any(|f| f.pattern_id == "BP-001"),
        "a struct-literal From with path fields is trivial"
    );
}

#[test]
fn test_bp001_struct_literal_with_rest_not_trivial() {
    // `Self { a, ..Default::default() }` has a rest (`..`), so it is NOT trivial;
    // guards the `s.rest.is_none() && …` conjunction.
    let code = "struct W { a: i32 } impl From<i32> for W { fn from(a: i32) -> Self { Self { a, ..Default::default() } } }";
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-001"),
        "a struct literal with `..rest` is not a trivial From"
    );
}

#[test]
fn test_bp001_requires_method_named_from() {
    // A single method NOT named `from` must not be treated as a From body; guards
    // the `methods.len() != 1 || ident != "from"` short-circuit.
    let code =
        "struct W { a: i32 } impl From<i32> for W { fn convert(a: i32) -> Self { Self { a } } }";
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-001"),
        "an impl From whose only method isn't `from` is not BP-001"
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

#[test]
fn test_bp005_recognises_each_default_value_kind() {
    // BP-005 fires only when *every* field is a default value, so each fixture's
    // single field uses a distinct default-value kind: dropping that kind's arm
    // in `is_default_value_expr` (or breaking its `==`) stops the impl flagging.
    let cases = [
        ("float 0.0", "struct S { x: f64 } impl Default for S { fn default() -> Self { Self { x: 0.0 } } }"),
        ("empty str", r#"struct S { x: String } impl Default for S { fn default() -> Self { Self { x: "" } } }"#),
        ("None", "struct S { x: Option<i32> } impl Default for S { fn default() -> Self { Self { x: None } } }"),
        ("empty vec!", "struct S { x: Vec<i32> } impl Default for S { fn default() -> Self { Self { x: vec![] } } }"),
    ];
    for (label, code) in cases {
        let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
        assert!(
            findings.iter().any(|f| f.pattern_id == "BP-005"),
            "default-value kind not recognised: {label}"
        );
    }
}

#[test]
fn test_bp005_reports_struct_name() {
    // The finding carries the impl's self-type name; guards `self_type_of`.
    let code = "struct Config { x: i32 } impl Default for Config { fn default() -> Self { Self { x: 0 } } }";
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    let bp005 = findings
        .iter()
        .find(|f| f.pattern_id == "BP-005")
        .expect("BP-005 finding");
    assert_eq!(bp005.struct_name.as_deref(), Some("Config"));
}

#[test]
fn test_bp005_requires_method_named_default() {
    // A single method NOT named `default` must not be treated as a Default impl
    // body. Guards the `methods.len() != 1 || ident != "default"` short-circuit.
    let code =
        "struct S { x: i32 } impl Default for S { fn not_default() -> Self { Self { x: 0 } } }";
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-005"),
        "an impl Default whose only method isn't `default` is not BP-005"
    );
}
