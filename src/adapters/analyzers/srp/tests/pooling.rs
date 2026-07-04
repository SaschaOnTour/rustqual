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

// ── Qualified-path impls must still pool with their struct ──────────────
//
// The owner-key must NOT diverge between a struct and an impl written with a
// *qualified* self-type (`inner::Foo`, `super::Foo`) relative to the current
// module. Reducing the impl's self-type to its bare last segment keyed it
// differently from the struct, so the methods stopped pooling and a real
// god-struct became a false negative. Shared fixture pieces (kept in consts so
// the near-identical bodies don't trip DRY) build a clearly incohesive struct:
// it must warn only when its methods pool with the qualified impl.

const GOD_FIELDS: &str = "db: u8, cache: u8, logger: u8, metrics: u8, config: u8, \
    state: u8, buffer: u8, queue: u8, pool: u8, handler: u8, router: u8, auth: u8";

const GOD_METHODS: &str = "
    fn read_db(&self) { query(self.db); }
    fn write_db(&mut self) { commit(self.db); }
    fn read_cache(&self) { get_key(self.cache); }
    fn write_cache(&mut self) { set_key(self.cache); }
    fn log_info(&self) { format_log(self.logger); }
    fn log_error(&self) { format_log(self.logger); inc(self.metrics); }
    fn route(&self) { dispatch(self.router, self.handler); }
    fn authenticate(&self) { verify(self.auth, self.config); }
    fn flush(&mut self) { drain(self.buffer, self.queue); }
    fn manage_pool(&mut self) { alloc(self.pool, self.state); }
";

/// Does the god-struct `GodFixture` in `code` trip SRP-001? (True only when its
/// methods pool with the struct — i.e. the owner keys match.)
fn god_fixture_warns(code: &str) -> bool {
    let parsed = vec![("t.rs".to_string(), code.to_string(), parse_file(code))];
    analyze_srp(
        &parsed,
        &SrpConfig::default(),
        &std::collections::HashMap::new(),
        300,
    )
    .struct_warnings
    .iter()
    .any(|w| w.struct_name == "GodFixture")
}

#[test]
fn bare_impl_god_struct_warns_baseline() {
    // Sanity: with the impl written as a bare `impl GodFixture`, the fixture is
    // a god-struct and SRP-001 fires — so the two qualified-path tests below
    // are exercising pooling, not a dud fixture.
    let code = format!("struct GodFixture {{ {GOD_FIELDS} }} impl GodFixture {{ {GOD_METHODS} }}");
    assert!(god_fixture_warns(&code), "bare-impl god-struct must warn");
}

#[test]
fn impl_qualified_into_inline_module_still_pools() {
    // `mod inner { struct GodFixture … }` with `impl inner::GodFixture` at the
    // file root: struct key is `t.rs::inner::GodFixture`; the impl's relative
    // `inner::` path must resolve to the same key so the methods pool.
    let code = format!(
        "mod inner {{ pub struct GodFixture {{ {GOD_FIELDS} }} }} \
         impl inner::GodFixture {{ {GOD_METHODS} }}"
    );
    assert!(
        god_fixture_warns(&code),
        "an `impl inner::Foo` must pool with the struct in `mod inner`"
    );
}

#[test]
fn impl_qualified_with_super_still_pools() {
    // Struct at the file root, impl inside `mod ops` written as
    // `impl super::GodFixture`: `super::` must climb back to the root so the
    // impl keys as `t.rs::GodFixture`, matching the struct.
    let code = format!(
        "struct GodFixture {{ {GOD_FIELDS} }} \
         mod ops {{ impl super::GodFixture {{ {GOD_METHODS} }} }}"
    );
    assert!(
        god_fixture_warns(&code),
        "an `impl super::Foo` must pool with the struct in the parent module"
    );
}

#[test]
fn impl_with_crate_absolute_path_is_accepted_under_report() {
    // Characterization (NOT aspiration): `crate::foo::Bar` names the CRATE
    // module hierarchy, but the owner key is `file + inline-module stack` and
    // does not model a file-backed module's crate path — resolving it would
    // need a fragile file-path→module-path derivation rustqual deliberately
    // avoids. So an absolute-path impl in a file-backed module does NOT pool
    // with its struct: an accepted safe-direction under-report (never a false
    // positive), pinned here so a future resolver that changes it is noticed.
    // The bare/relative forms above are the supported cases.
    let code = format!(
        "struct GodFixture {{ {GOD_FIELDS} }} impl crate::foo::GodFixture {{ {GOD_METHODS} }}"
    );
    let parsed = vec![("src/foo.rs".to_string(), code.clone(), parse_file(&code))];
    let warns = analyze_srp(
        &parsed,
        &SrpConfig::default(),
        &std::collections::HashMap::new(),
        300,
    )
    .struct_warnings
    .iter()
    .any(|w| w.struct_name == "GodFixture");
    assert!(
        !warns,
        "absolute `crate::` path impl is an accepted under-report, not a pool"
    );
}
