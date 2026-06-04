//! Predicate edge-case tests for the boilerplate detectors that the main
//! BP-NNN files (near the file-length cap) have no room for. Each pins a
//! specific guard/branch in a detector's recognition predicate.
use super::*;

#[test]
fn test_bp004_builder_method_must_return_bare_self() {
    // A builder method's second statement must be the bare `self` value. These
    // methods assign a field but return `Self::new()` instead, so they are NOT
    // builders — guards the `has_assign && returns_self_val` conjunction
    // (turning it into `||` would accept them on the assignment alone).
    let code = r#"
        struct B { a: i32, b: i32, c: i32 }
        impl B {
            fn with_a(mut self, v: i32) -> Self { self.a = v; Self::new() }
            fn with_b(mut self, v: i32) -> Self { self.b = v; Self::new() }
            fn with_c(mut self, v: i32) -> Self { self.c = v; Self::new() }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-004"),
        "methods that assign but do not return bare self are not builders"
    );
}

#[test]
fn test_bp010_short_format_strings_not_collected() {
    // Format strings shorter than the minimum length must not be collected, so
    // repeating a short one (`"ab"`, len 4 < 5) does not trip BP-010. Guards the
    // `starts_with('"') && s.len() >= MIN_FORMAT_STRING_LEN` conjunction.
    let code = r#"
        fn f() {
            println!("ab");
            println!("ab");
            println!("ab");
            println!("ab");
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-010"),
        "short format strings (below the length minimum) must not be collected"
    );
}

#[test]
fn test_bp009_low_field_overlap_not_flagged() {
    // Two constructions of `Config` sharing only 1 of 3 field names → overlap
    // ratio 1/3 < 0.5, below the threshold, so NOT flagged. Guards the
    // `overlap / min_len >= MIN_OVERLAP_RATIO` division (a `*` would always pass).
    let code = r#"
        struct Config { a: i32, b: i32, c: i32, x: i32, y: i32 }
        fn make() -> (Config, Config) {
            let p = Config { a: 1, b: 2, c: 3 };
            let q = Config { a: 1, x: 2, y: 3 };
            (p, q)
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-009"),
        "constructions with overlap ratio below 0.5 must not be flagged"
    );
}

#[test]
fn test_bp007_from_impl_with_extra_method_not_counted() {
    // An `impl From` is only an error-enum candidate when it has exactly ONE
    // method named `from`. Three impls each with a second method must NOT be
    // counted — guards the `methods.len() == 1 && ident == "from"` conjunction.
    let code = r#"
        enum E {}
        impl From<i32> for E { fn from(x: i32) -> Self { E::from(0) } fn extra() {} }
        impl From<u8> for E { fn from(x: u8) -> Self { E::from(0) } fn extra() {} }
        impl From<u16> for E { fn from(x: u16) -> Self { E::from(0) } fn extra() {} }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-007"),
        "From impls with a second method are not error-enum candidates"
    );
}

#[test]
fn test_bp007_from_impl_with_non_expr_body_not_counted() {
    // A candidate `from` body must be a single trailing expression. Three impls
    // whose `from` body is a single `let` statement must NOT count — guards the
    // `stmts.len() == 1 && matches!(Stmt::Expr(_, None))` conjunction.
    let code = r#"
        enum E {}
        impl From<i32> for E { fn from(x: i32) -> Self { let _y = x; } }
        impl From<u8> for E { fn from(x: u8) -> Self { let _y = x; } }
        impl From<u16> for E { fn from(x: u16) -> Self { let _y = x; } }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-007"),
        "From impls whose body is a let-statement are not trivial wrapping"
    );
}

#[test]
fn test_bp006_tuple_struct_variant_arms_detected() {
    // A repetitive enum→enum match whose arms are tuple-struct variant patterns
    // (`Msg::A(_) => Out::A`) must be detected; guards the `Pat::TupleStruct` arm
    // of `is_simple_enum_pattern`.
    let code = r#"
        enum Msg { A(i32), B(i32), C(i32), D(i32) }
        enum Out { A, B, C, D }
        fn convert(m: Msg) -> Out {
            match m {
                Msg::A(_) => Out::A,
                Msg::B(_) => Out::B,
                Msg::C(_) => Out::C,
                Msg::D(_) => Out::D,
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        findings.iter().any(|f| f.pattern_id == "BP-006"),
        "a tuple-struct-variant enum mapping is detected"
    );
}
