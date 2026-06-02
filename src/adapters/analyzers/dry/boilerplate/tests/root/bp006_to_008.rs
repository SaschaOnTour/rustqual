use super::*;

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
