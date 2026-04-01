use crate::config::sections::BoilerplateConfig;

// ── Result type ────────────────────────────────────────────────

/// A boilerplate pattern finding.
#[derive(Debug, Clone)]
pub struct BoilerplateFind {
    pub pattern_id: String,
    pub file: String,
    pub line: usize,
    pub struct_name: Option<String>,
    pub description: String,
    pub suggestion: String,
}

// ── Pattern enablement macro ───────────────────────────────────

/// Early-return if the given pattern ID is disabled in config.
macro_rules! pattern_guard {
    ($id:expr, $config:expr) => {
        if !$config.patterns.is_empty() && $config.patterns.iter().all(|p| p != $id) {
            return vec![];
        }
    };
}

// ── Helpers (called only from within closures for IOSP) ────────

pub(crate) fn trait_name_of(imp: &syn::ItemImpl) -> Option<String> {
    imp.trait_
        .as_ref()
        .and_then(|(_, path, _)| path.segments.last().map(|s| s.ident.to_string()))
}

pub(crate) fn self_type_of(imp: &syn::ItemImpl) -> Option<String> {
    if let syn::Type::Path(tp) = &*imp.self_ty {
        tp.path.segments.last().map(|s| s.ident.to_string())
    } else {
        None
    }
}

pub(crate) fn single_return_expr(block: &syn::Block) -> Option<&syn::Expr> {
    if block.stmts.len() == 1 {
        if let syn::Stmt::Expr(expr, None) = &block.stmts[0] {
            return Some(expr);
        }
    }
    None
}

pub(crate) fn is_self_field_access(expr: &syn::Expr) -> bool {
    if let syn::Expr::Field(f) = expr {
        if let syn::Expr::Path(p) = &*f.base {
            return p.path.segments.last().is_some_and(|s| s.ident == "self");
        }
    }
    false
}

pub(crate) fn is_default_value_expr(expr: &syn::Expr) -> bool {
    match expr {
        syn::Expr::Lit(lit) => match &lit.lit {
            syn::Lit::Int(i) => i.base10_parse::<i64>().ok() == Some(0),
            syn::Lit::Float(f) => f.base10_parse::<f64>().ok() == Some(0.0),
            syn::Lit::Bool(b) => !b.value,
            syn::Lit::Str(s) => s.value().is_empty(),
            _ => false,
        },
        syn::Expr::Path(p) => p.path.segments.last().is_some_and(|s| s.ident == "None"),
        syn::Expr::Call(call) => {
            if let syn::Expr::Path(p) = &*call.func {
                let segs: Vec<_> = p
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect();
                matches!(
                    segs.iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .as_slice(),
                    ["Default", "default"]
                        | ["String", "new"]
                        | ["Vec", "new"]
                        | ["HashMap", "new"]
                        | ["HashSet", "new"]
                        | ["BTreeMap", "new"]
                        | ["BTreeSet", "new"]
                )
            } else {
                false
            }
        }
        syn::Expr::Macro(m) => {
            let name = m
                .mac
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default();
            name == "vec" && m.mac.tokens.is_empty()
        }
        _ => false,
    }
}

/// Check if a match arm pattern is a simple enum variant pattern suitable for
/// repetitive enum mapping detection. Accepts unit variants (`Color::Red`) and
/// tuple-struct variants with only wildcard sub-patterns (`Action::Add(_)`).
/// Rejects or-patterns, top-level wildcards, tuple patterns, and variable bindings.
/// Operation: pattern matching, no own calls.
fn is_simple_enum_pattern(pat: &syn::Pat) -> bool {
    match pat {
        // Unit variant: `Color::Red`
        syn::Pat::Path(_) => true,
        // Tuple-struct variant: `Action::Add(_)` — only if all sub-patterns are wildcards
        syn::Pat::TupleStruct(ts) => ts.elems.iter().all(|p| matches!(p, syn::Pat::Wild(_))),
        // Struct variant: `Msg { .. }` — only if fields are empty (rest-only)
        syn::Pat::Struct(ps) => ps.fields.is_empty(),
        _ => false,
    }
}

/// Check if all match arms represent a repetitive enum-to-enum mapping.
/// Arms must have simple enum variant patterns (no or-patterns, wildcards, bindings)
/// and path or call bodies. No guards allowed.
/// Operation: iterates arms checking pattern + body constraints.
pub(crate) fn is_repetitive_enum_mapping(arms: &[syn::Arm]) -> bool {
    arms.iter().all(|arm| {
        // Guard expressions disqualify
        if arm.guard.is_some() {
            return false;
        }
        // Pattern must be a simple enum variant
        is_simple_enum_pattern(&arm.pat)
            // Body must be a path expression (enum variant) or call
            && matches!(&*arm.body, syn::Expr::Path(_) | syn::Expr::Call(_))
    })
}

pub(crate) fn count_field_clones(expr: &syn::Expr) -> usize {
    if let syn::Expr::Struct(s) = expr {
        s.fields
            .iter()
            .filter(|f| {
                matches!(&f.expr, syn::Expr::MethodCall(mc) if mc.method == "clone" && mc.args.is_empty())
            })
            .count()
    } else {
        0
    }
}

// ── Pattern modules ────────────────────────────────────────────

mod builder;
mod clone_conversion;
mod error_enum;
mod format_repetition;
mod getter_setter;
mod manual_default;
mod repetitive_match;
mod struct_update;
mod trivial_display;
mod trivial_from;

// ── Detection API ──────────────────────────────────────────────

/// Detect boilerplate patterns across parsed files.
/// Integration: orchestrates all 10 pattern checkers.
pub fn detect_boilerplate(
    parsed: &[(String, String, syn::File)],
    config: &BoilerplateConfig,
) -> Vec<BoilerplateFind> {
    let mut findings = trivial_from::check_trivial_from(parsed, config);
    findings.extend(trivial_display::check_trivial_display(parsed, config));
    findings.extend(getter_setter::check_manual_getter_setter(parsed, config));
    findings.extend(builder::check_builder_boilerplate(parsed, config));
    findings.extend(manual_default::check_manual_default(parsed, config));
    findings.extend(repetitive_match::check_repetitive_match(parsed, config));
    findings.extend(error_enum::check_error_enum_boilerplate(parsed, config));
    findings.extend(clone_conversion::check_clone_heavy_conversion(
        parsed, config,
    ));
    findings.extend(struct_update::check_repetitive_struct_update(
        parsed, config,
    ));
    findings.extend(format_repetition::check_format_repetition(parsed, config));
    findings
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
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
}
