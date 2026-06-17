//! RQ-1 regression: same-named structs in different files / inline modules
//! must not pool their impl methods into one cohesion bucket. Lives in its
//! own file so `tests/root.rs` stays under the SRP module-length cap.

use crate::adapters::analyzers::srp::analyze_srp;
use crate::config::sections::SrpConfig;

fn parse_file(code: &str) -> syn::File {
    syn::parse_file(code).expect("Failed to parse test code")
}

/// RQ-1 (sovard `docs/tools/rustqual-followups.md`): `build_struct_warnings`
/// pools methods by the BARE last path segment of the type name, so two
/// same-named structs in different files/crates share one method bucket. Each
/// `Dup` here is individually cohesive (every method touches its one field) —
/// neither warns alone — but analyzed together the file-A `Dup` is scored
/// against the *pooled* methods of both: the foreign-field methods are
/// fieldless, isolated LCOM4 components, inflating LCOM4 + method-count +
/// fan-out past the smell threshold. A struct's cohesion must not depend on
/// whether an unrelated crate happens to reuse the name.
#[test]
fn test_analyze_srp_does_not_pool_methods_across_same_named_structs() {
    let code_a = r#"
        struct Dup { a: i32 }
        impl Dup {
            fn ga1(&self) { fa1(self.a); }
            fn ga2(&self) { fa2(self.a); }
            fn ga3(&self) { fa3(self.a); }
            fn ga4(&self) { fa4(self.a); }
            fn ga5(&self) { fa5(self.a); }
            fn ga6(&self) { fa6(self.a); }
        }
    "#;
    let code_b = r#"
        struct Dup { z: i32 }
        impl Dup {
            fn gb1(&self) { fb1(self.z); }
            fn gb2(&self) { fb2(self.z); }
            fn gb3(&self) { fb3(self.z); }
            fn gb4(&self) { fb4(self.z); }
            fn gb5(&self) { fb5(self.z); }
            fn gb6(&self) { fb6(self.z); }
        }
    "#;
    let config = SrpConfig::default();
    let empty = std::collections::HashMap::new();

    // Baseline: each cohesive `Dup` is clean when analyzed on its own.
    let only_a = vec![("a.rs".to_string(), code_a.to_string(), parse_file(code_a))];
    let only_b = vec![("b.rs".to_string(), code_b.to_string(), parse_file(code_b))];
    assert!(
        analyze_srp(&only_a, &config, &empty, 300)
            .struct_warnings
            .is_empty(),
        "file A's Dup is cohesive on its own — must not warn"
    );
    assert!(
        analyze_srp(&only_b, &config, &empty, 300)
            .struct_warnings
            .is_empty(),
        "file B's Dup is cohesive on its own — must not warn"
    );

    // Together: neither struct changed, so neither should suddenly trip SRP.
    let both = vec![
        ("a.rs".to_string(), code_a.to_string(), parse_file(code_a)),
        ("b.rs".to_string(), code_b.to_string(), parse_file(code_b)),
    ];
    let warnings = analyze_srp(&both, &config, &empty, 300).struct_warnings;
    assert!(
        !warnings.iter().any(|w| w.struct_name == "Dup"),
        "same-named structs in different files must not pool methods; got {warnings:?}"
    );
}
