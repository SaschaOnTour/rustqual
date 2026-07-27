use crate::adapters::analyzers::dry::functions::*;
use crate::adapters::analyzers::dry::FunctionHashEntry;
use crate::config::sections::DuplicatesConfig;

fn parse(code: &str) -> Vec<(String, String, syn::File)> {
    let syntax = syn::parse_file(code).expect("parse failed");
    vec![("test.rs".to_string(), code.to_string(), syntax)]
}

fn parse_multi(files: &[(&str, &str)]) -> Vec<(String, String, syn::File)> {
    files
        .iter()
        .map(|(name, code)| {
            let syntax = syn::parse_file(code).expect("parse failed");
            (name.to_string(), code.to_string(), syntax)
        })
        .collect()
}

const TEST_MIN_TOKENS: usize = 3;

fn low_threshold_config() -> DuplicatesConfig {
    DuplicatesConfig {
        min_tokens: TEST_MIN_TOKENS,
        min_lines: 1,
        ..DuplicatesConfig::default()
    }
}

#[test]
fn test_detect_duplicates_no_functions() {
    let parsed = parse("");
    let config = low_threshold_config();
    let groups = detect_duplicates(&parsed, &config);
    assert!(groups.is_empty());
}

#[test]
fn test_detect_duplicates_no_duplicates() {
    let code = r#"
        fn foo() { let x = 1; let y = x + 2; let z = y * x; }
        fn bar() { let a = "hello"; let b = a.len(); if b > 0 { return; } }
    "#;
    let parsed = parse(code);
    let config = low_threshold_config();
    let groups = detect_duplicates(&parsed, &config);
    assert!(
        groups.is_empty(),
        "Different functions should not be duplicates"
    );
}

#[test]
fn test_detect_exact_duplicates_same_structure() {
    // Two functions with identical structure but different variable names
    let parsed = parse_multi(&[
        (
            "a.rs",
            "fn process_a() { let x = 1; let y = x + 2; let z = y * x; }",
        ),
        (
            "b.rs",
            "fn process_b() { let a = 1; let b = a + 2; let c = b * a; }",
        ),
    ]);
    let config = low_threshold_config();
    let groups = detect_duplicates(&parsed, &config);
    assert_eq!(groups.len(), 1, "Should detect one exact duplicate group");
    assert!(matches!(groups[0].kind, DuplicateKind::Exact));
    assert_eq!(groups[0].entries.len(), 2);
}

#[test]
fn test_detect_duplicates_different_structure() {
    let parsed = parse_multi(&[
        (
            "a.rs",
            "fn add() { let x = 1; let y = x + 2; let z = y + x; }",
        ),
        (
            "b.rs",
            "fn mul() { let a = 1; let b = a * 2; let c = b * a; }",
        ),
    ]);
    let config = low_threshold_config();
    let groups = detect_duplicates(&parsed, &config);
    // Different operators → different hash → no exact duplicate
    let exact_groups: Vec<_> = groups
        .iter()
        .filter(|g| matches!(g.kind, DuplicateKind::Exact))
        .collect();
    assert!(
        exact_groups.is_empty(),
        "Different operators should not be exact duplicates"
    );
}

#[test]
fn test_detect_duplicates_below_min_tokens_excluded() {
    let parsed = parse_multi(&[
        ("a.rs", "fn tiny_a() { let x = 1; }"),
        ("b.rs", "fn tiny_b() { let y = 1; }"),
    ]);
    let config = DuplicatesConfig::default(); // min_tokens = 30
    let groups = detect_duplicates(&parsed, &config);
    assert!(
        groups.is_empty(),
        "Small functions below min_tokens should be excluded"
    );
}

#[test]
fn test_detect_duplicates_includes_test_functions() {
    // Duplicate detection always runs on test code (the `ignore_tests`
    // toggle was removed in v1.4.0) — duplicate `#[cfg(test)]` helpers are
    // flagged just like production duplicates.
    let code = r#"
        #[cfg(test)]
        mod tests {
            fn helper_a() { let x = 1; let y = x + 2; let z = y * x; }
            fn helper_b() { let a = 1; let b = a + 2; let c = b * a; }
        }
    "#;
    let parsed = parse(code);
    let groups = detect_duplicates(&parsed, &low_threshold_config());
    assert_eq!(groups.len(), 1, "test functions must be included");
}

#[test]
fn duplicates_in_cfg_test_companion_file_flagged() {
    // A `#![cfg(test)]` companion file is test code even though its path
    // has no `tests/` segment. Since v1.4.0 duplicate detection runs on
    // tests too, so its duplicate helpers ARE flagged.
    let code = r#"
        #![cfg(test)]
        fn helper_one() { let x = 1; let y = x + 2; let z = y * x; }
        fn helper_two() { let a = 1; let b = a + 2; let c = b * a; }
    "#;
    let parsed = vec![(
        "src/foo_tests.rs".to_string(),
        code.to_string(),
        syn::parse_file(code).expect("parse failed"),
    )];
    let groups = detect_duplicates(&parsed, &low_threshold_config());
    assert_eq!(
        groups.len(),
        1,
        "duplicates in a #![cfg(test)] file must be flagged: {groups:?}"
    );
}

#[test]
fn duplicates_in_production_file_under_nested_tests_dir_still_flagged() {
    // Inverse of the review fix: a `src/**/tests/` directory that is NOT
    // reached via `#[cfg(test)] mod` is production code and its
    // duplicates must still be flagged (the old broad path heuristic
    // wrongly skipped anything containing `/tests/`).
    let parsed = parse_multi(&[
        (
            "src/database/tests/conn_a.rs",
            "pub fn conn_a() { let x = 1; let y = x + 2; let z = y * x; }",
        ),
        (
            "src/database/tests/conn_b.rs",
            "pub fn conn_b() { let a = 1; let b = a + 2; let c = b * a; }",
        ),
    ]);
    let config = low_threshold_config();
    let groups = detect_duplicates(&parsed, &config);
    assert_eq!(
        groups.len(),
        1,
        "duplicates in a non-cfg-test src/**/tests/ file must be flagged: {groups:?}"
    );
}

#[test]
fn test_detect_duplicates_three_way() {
    let parsed = super::three_matching_funcs();
    let groups = detect_duplicates(&parsed, &low_threshold_config());
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].entries.len(), 3, "Should detect 3-way duplicate");
}

#[test]
fn test_detect_duplicates_config_disabled() {
    let parsed = parse_multi(&[
        (
            "a.rs",
            "fn func_a() { let x = 1; let y = x + 2; let z = y * x; }",
        ),
        (
            "b.rs",
            "fn func_b() { let a = 1; let b = a + 2; let c = b * a; }",
        ),
    ]);
    let mut config = low_threshold_config();
    config.enabled = false;
    // detect_duplicates doesn't check enabled — pipeline does.
    // But we can test it still works.
    let groups = detect_duplicates(&parsed, &config);
    assert!(!groups.is_empty());
}

#[test]
fn test_group_exact_duplicates_returns_remaining() {
    let entries = vec![
        FunctionHashEntry {
            name: "a".into(),
            qualified_name: "a".into(),
            file: "a.rs".into(),
            line: 1,
            hash: 100,
            token_count: 10,
            tokens: vec![],
        },
        FunctionHashEntry {
            name: "b".into(),
            qualified_name: "b".into(),
            file: "b.rs".into(),
            line: 1,
            hash: 200,
            token_count: 10,
            tokens: vec![],
        },
        FunctionHashEntry {
            name: "c".into(),
            qualified_name: "c".into(),
            file: "c.rs".into(),
            line: 1,
            hash: 100,
            token_count: 10,
            tokens: vec![],
        },
    ];
    let (groups, remaining) = group_exact_duplicates(&entries);
    assert_eq!(groups.len(), 1); // a and c share hash 100
    assert_eq!(remaining.len(), 1); // b is alone
    assert_eq!(remaining[0], 1); // index of b
}

/// Count the `NearDuplicate` groups in `parsed` at the given similarity
/// threshold, plus the first one's similarity. Operation: filter + project.
fn near_dup_similarity(
    parsed: &[(String, String, syn::File)],
    threshold: f64,
) -> (usize, Option<f64>) {
    let mut config = low_threshold_config();
    config.similarity_threshold = threshold;
    let groups = detect_duplicates(parsed, &config);
    let near: Vec<f64> = groups
        .iter()
        .filter_map(|g| match g.kind {
            DuplicateKind::NearDuplicate { similarity } => Some(similarity),
            _ => None,
        })
        .collect();
    (near.len(), near.first().copied())
}

#[test]
fn near_duplicate_detected_above_threshold_but_rejected_below_it() {
    // `func_a`/`func_b` are token-identical except `+ 1` vs `- 1` (same token
    // count → same bucket → comparable), forming a near- (not exact)
    // duplicate. At a permissive threshold it IS flagged; raising the
    // threshold strictly ABOVE the pair's measured similarity rejects it.
    // Pins both halves of the spec's "deliberately strict" contract — the old
    // version asserted similarity only INSIDE an `if detected`, so it passed
    // vacuously when detection broke.
    let parsed = parse_multi(&[
        (
            "a.rs",
            "fn func_a() { let x = 1; let y = x + 2; let z = y * x; let w = z + 1; }",
        ),
        (
            "b.rs",
            "fn func_b() { let x = 1; let y = x + 2; let z = y * x; let w = z - 1; }",
        ),
    ]);
    let (count, similarity) = near_dup_similarity(&parsed, 0.50);
    assert_eq!(count, 1, "permissive threshold detects the near-duplicate");
    let similarity = similarity.unwrap();
    assert!(
        (0.50..1.0).contains(&similarity),
        "near (not exact) similarity in [0.50, 1.0): {similarity}"
    );
    let strict = (similarity + 1.0) / 2.0;
    let (strict_count, _) = near_dup_similarity(&parsed, strict);
    assert_eq!(
        strict_count, 0,
        "threshold {strict} above similarity {similarity} rejects the pair"
    );
}

#[test]
fn test_duplicate_entry_has_file_and_line() {
    let parsed = parse_multi(&[
        (
            "module_a.rs",
            "fn func_a() { let x = 1; let y = x + 2; let z = y * x; }",
        ),
        (
            "module_b.rs",
            "fn func_b() { let a = 1; let b = a + 2; let c = b * a; }",
        ),
    ]);
    let config = low_threshold_config();
    let groups = detect_duplicates(&parsed, &config);
    assert!(!groups.is_empty());
    for entry in &groups[0].entries {
        assert!(!entry.file.is_empty());
        assert!(!entry.name.is_empty());
    }
}

#[test]
fn token_count_exactly_at_min_tokens_is_kept() {
    // `let x = 1;` normalises to exactly 5 tokens. With `min_tokens = 5` it is
    // at the boundary and must be KEPT (`tokens.len() < min_tokens` is false).
    // Guards the `<` in `build_hash_entry` (a `<=` would drop it).
    let config = DuplicatesConfig {
        min_tokens: 5,
        min_lines: 1,
        ..DuplicatesConfig::default()
    };
    let parsed = parse_multi(&[
        ("a.rs", "fn a() { let x = 1; }"),
        ("b.rs", "fn b() { let y = 1; }"),
    ]);
    let groups = detect_duplicates(&parsed, &config);
    assert_eq!(
        groups.len(),
        1,
        "bodies with exactly min_tokens tokens are kept and form a duplicate, got {groups:?}"
    );
}

#[test]
fn bodies_calling_different_functions_are_not_duplicates() {
    // The manifest shape: a suite runner whose whole meaning is *which*
    // functions it names. Normalising a callee to a positional index made every
    // such runner an exact duplicate of every other one of the same length —
    // and the two checks then pulled against each other, DRY-002 rewarding the
    // spelled-out calls that DRY-001 punished.
    let code = r#"
fn run_audit(s: &S) {
    check_append(s);
    check_rotate(s);
    check_prune(s);
}
fn run_snapshot(s: &S) {
    check_store(s);
    check_load(s);
    check_evict(s);
}
"#;
    let groups = detect_duplicates(&parse(code), &low_threshold_config());
    assert!(
        groups.is_empty(),
        "disjoint callees are not the same function: {groups:?}"
    );
}

#[test]
fn bodies_calling_the_same_functions_are_still_duplicates() {
    // The counterpart: keeping the callee names must not blind the check to a
    // real copy. Locals and parameters stay alpha-renamed, so only the names
    // that carry the meaning have to match.
    let code = r#"
fn run_here(store: &S) {
    check_append(store);
    check_rotate(store);
}
fn run_there(other: &S) {
    check_append(other);
    check_rotate(other);
}
"#;
    let groups = detect_duplicates(&parse(code), &low_threshold_config());
    assert_eq!(groups.len(), 1, "same callees, same body: {groups:?}");
}

#[test]
fn a_qualified_callee_is_still_dropped_entirely() {
    // Known limit, pinned so it is visible rather than folklore: a
    // multi-segment callee contributes no token at all, so two bodies made of
    // qualified calls to entirely different functions still match. Naming them
    // by their last segment would conflate `Config::default()` with
    // `Summary::default()`, and emitting any token raises the count of every
    // body that calls a qualified function — which shifts what clears
    // `min_tokens`. That is its own change.
    let code = r#"
fn run_audit(s: &S) {
    audit::check_append(s);
    audit::check_rotate(s);
}
fn run_snapshot(s: &S) {
    snapshot::check_store(s);
    snapshot::check_load(s);
}
"#;
    let groups = detect_duplicates(&parse(code), &low_threshold_config());
    assert_eq!(groups.len(), 1, "the limit, not the goal: {groups:?}");
}

#[test]
fn a_callback_parameter_is_still_alpha_renamed() {
    // Keeping the callee's name must not reach the case where the callee *is*
    // a local: `f` and `callback` are the same parameter under two spellings,
    // and the promise this check makes about locals and parameters has to hold
    // in call position too. Otherwise two identical higher-order functions
    // drift apart on a rename alone.
    let code = r#"
fn apply_here(f: F, x: u8, y: u8) {
    f(x);
    f(y);
    f(x);
}
fn apply_there(callback: F, x: u8, y: u8) {
    callback(x);
    callback(y);
    callback(x);
}
"#;
    let groups = detect_duplicates(&parse(code), &low_threshold_config());
    assert_eq!(groups.len(), 1, "a rename is not a difference: {groups:?}");
}

#[test]
fn a_let_bound_callee_is_still_alpha_renamed() {
    // The same for a local binding, which needs no signature to see.
    let code = r#"
fn run_here() {
    let step = pick();
    step(1);
    step(2);
}
fn run_there() {
    let chosen = pick();
    chosen(1);
    chosen(2);
}
"#;
    let groups = detect_duplicates(&parse(code), &low_threshold_config());
    assert_eq!(groups.len(), 1, "{groups:?}");
}
