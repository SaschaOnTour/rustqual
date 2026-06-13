//! BP-002 semantic-triviality tests: the check matches what the body *means*
//! (branch-free, write-only fmt), not which surface syntax it uses — plus the
//! accepted_display_idioms policy gate and the all-fix-forms suggestion.

use super::*;

/// A `BoilerplateConfig` with the given accepted Display idioms.
fn cfg_accepting(idioms: &[&str]) -> BoilerplateConfig {
    BoilerplateConfig {
        accepted_display_idioms: idioms.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

const WRITE_STR_NEWTYPE: &str = r#"
    use std::fmt;
    struct Id(String);
    impl fmt::Display for Id {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.0)
        }
    }
"#;

const WRITE_MACRO_NEWTYPE: &str = r#"
    use std::fmt;
    struct Id(String);
    impl fmt::Display for Id {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.0)
        }
    }
"#;

#[test]
fn test_bp002_write_str_newtype_detected() {
    // f.write_str(&self.0) is semantically the same trivial Display as
    // write!(f, "{}", self.0) — surface syntax must not decide the finding.
    let findings = detect_boilerplate(&parse(WRITE_STR_NEWTYPE), &BoilerplateConfig::default());
    assert!(
        findings.iter().any(|f| f.pattern_id == "BP-002"),
        "a write_str-only fmt body is trivial Display"
    );
}

#[test]
fn test_bp002_multi_statement_write_char_body_detected() {
    // The mechanical evasion rewrite observed in the wild: one write! split
    // into two write_char statements. Still branch-free, still write-only —
    // must still be BP-002 so the rule forces a decision, not a workaround.
    let code = r#"
        use std::fmt;
        struct Brackets;
        impl fmt::Display for Brackets {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_char('[')?;
                f.write_char(']')
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        findings.iter().any(|f| f.pattern_id == "BP-002"),
        "a multi-statement write_char body is still trivial Display"
    );
}

#[test]
fn test_bp002_accepted_idiom_is_policy_not_finding() {
    // Declaring write_str as the house idiom silences exactly that form …
    let cfg = cfg_accepting(&["write_str"]);
    let findings = detect_boilerplate(&parse(WRITE_STR_NEWTYPE), &cfg);
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-002"),
        "the declared house idiom is policy, not boilerplate"
    );
    // … while the write! form keeps firing under the same config — the rule
    // re-targets to consistency enforcement instead of going silent.
    let findings = detect_boilerplate(&parse(WRITE_MACRO_NEWTYPE), &cfg);
    assert!(
        findings.iter().any(|f| f.pattern_id == "BP-002"),
        "a non-accepted trivial form must still fire"
    );
}

#[test]
fn test_bp002_mixed_idioms_fire_unless_all_accepted() {
    let code = r#"
        use std::fmt;
        struct Tagged(String);
        impl fmt::Display for Tagged {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("tag:")?;
                write!(f, "{}", self.0)
            }
        }
    "#;
    let only_write_str = cfg_accepting(&["write_str"]);
    let findings = detect_boilerplate(&parse(code), &only_write_str);
    assert!(
        findings.iter().any(|f| f.pattern_id == "BP-002"),
        "a body mixing an accepted and a non-accepted idiom must fire"
    );
    let both = cfg_accepting(&["write_str", "write_macro"]);
    let findings = detect_boilerplate(&parse(code), &both);
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-002"),
        "every used idiom accepted ⇒ policy, no finding"
    );
}

#[test]
fn test_bp002_delegation_form_detected_and_acceptable() {
    let code = r#"
        use std::fmt;
        struct Wrapper(inner::Type);
        impl fmt::Display for Wrapper {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        findings.iter().any(|f| f.pattern_id == "BP-002"),
        "a pure Display::fmt delegation is trivial Display"
    );
    let findings = detect_boilerplate(&parse(code), &cfg_accepting(&["delegation"]));
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-002"),
        "delegation declared as house idiom ⇒ no finding"
    );
}

#[test]
fn test_bp002_local_binding_makes_body_non_trivial() {
    // A let-binding means the body computes something — not pure write-only
    // boilerplate, so the (conservative) check stays silent.
    let code = r#"
        use std::fmt;
        struct Id(String);
        impl fmt::Display for Id {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let upper = self.0.to_uppercase();
                f.write_str(&upper)
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-002"),
        "a body with a local binding is not trivial"
    );
}

#[test]
fn test_bp002_write_on_non_formatter_receiver_not_trivial() {
    // self.buf.write_str(…) writes somewhere else — that is real logic, not
    // a formatter-forwarding idiom; guards the receiver check.
    let code = r#"
        use std::fmt;
        struct Tee { buf: String }
        impl fmt::Display for Tee {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.buf.write_str("x")
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-002"),
        "write_str on a non-formatter receiver is not the trivial idiom"
    );
}

#[test]
fn test_bp002_suggestion_names_all_fix_forms() {
    // Workspace availability ≠ usability under a dependency policy: the
    // suggestion must always name every fix form so the consumer can pick
    // the one their project allows.
    let findings = detect_boilerplate(&parse(WRITE_MACRO_NEWTYPE), &BoilerplateConfig::default());
    let bp002 = findings
        .iter()
        .find(|f| f.pattern_id == "BP-002")
        .expect("BP-002 fires");
    for needle in ["derive_more", "accepted_display_idioms", "macro_rules"] {
        assert!(
            bp002.suggestion.contains(needle),
            "suggestion must name fix form `{needle}`: {}",
            bp002.suggestion
        );
    }
    // suggest_crates = false drops the crate name but keeps the derive form.
    let no_crates = BoilerplateConfig {
        suggest_crates: false,
        ..Default::default()
    };
    let findings = detect_boilerplate(&parse(WRITE_MACRO_NEWTYPE), &no_crates);
    let bp002 = findings
        .iter()
        .find(|f| f.pattern_id == "BP-002")
        .expect("BP-002 fires");
    assert!(
        !bp002.suggestion.contains("derive_more"),
        "suggest_crates = false must not name crates: {}",
        bp002.suggestion
    );
    assert!(
        bp002.suggestion.to_lowercase().contains("derive"),
        "the derive form itself must stay named: {}",
        bp002.suggestion
    );
}

#[test]
fn test_bp002_write_macro_on_non_formatter_target_not_trivial() {
    // write!(self.buf, …) writes somewhere else — real logic, not the
    // documented write!(f, …) idiom. It must neither fire BP-002 (not
    // derivable) nor count as `write_macro` (else
    // accepted_display_idioms = ["write_macro"] would silently bless it).
    let code = r#"
        use std::fmt;
        struct Tee { buf: String }
        impl fmt::Display for Tee {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(self.buf, "x")
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        !findings.iter().any(|f| f.pattern_id == "BP-002"),
        "write! on a non-formatter target is not the trivial idiom"
    );
}

#[test]
fn test_bp002_write_macro_checks_formatter_even_when_renamed() {
    // The formatter param is positional, not nominal: `fmt` instead of `f`
    // must still classify write!(fmt, …) as the write_macro idiom.
    let code = r#"
        use std::fmt;
        struct Id(String);
        impl fmt::Display for Id {
            fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(fmt, "{}", self.0)
            }
        }
    "#;
    let findings = detect_boilerplate(&parse(code), &BoilerplateConfig::default());
    assert!(
        findings.iter().any(|f| f.pattern_id == "BP-002"),
        "write!(renamed_formatter, …) is still the trivial idiom"
    );
}
