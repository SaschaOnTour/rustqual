//! `run_dry_detection` per-detector enable guards. Each closure has an inner
//! `if !config.<flag> { return vec![]; }` short-circuit; deleting the `!` would
//! run the detector when the flag is off.
use super::*;

fn parse(code: &str) -> Vec<(String, String, syn::File)> {
    vec![(
        "test.rs".to_string(),
        code.to_string(),
        syn::parse_file(code).expect("parse failed"),
    )]
}

fn run_dry(config: &crate::config::Config, code: &str) -> DryResults {
    let parsed = parse(code);
    let sups = std::collections::HashMap::new();
    let api = std::collections::HashMap::new();
    let test_helper = std::collections::HashMap::new();
    let annotation_lines = AnnotationLines {
        api: &api,
        test_helper: &test_helper,
    };
    let cfg_test_files = std::collections::HashSet::new();
    run_dry_detection(&parsed, config, &sups, &annotation_lines, &cfg_test_files)
}

#[test]
fn dead_code_detection_skipped_when_disabled() {
    // With `detect_dead_code = false` an uncalled fn must NOT be reported;
    // guards the `if !c.duplicates.detect_dead_code` short-circuit.
    let mut config = crate::config::Config::default();
    config.duplicates.detect_dead_code = false;
    config.compile();
    let dry = run_dry(&config, "fn unused() { let _ = 1; }");
    assert!(
        dry.dead_code.is_empty(),
        "no dead-code warnings when detect_dead_code is off, got {:?}",
        dry.dead_code
    );
}

#[test]
fn dead_type_detection_skipped_when_disabled() {
    // With `detect_dead_types = false` an unreferenced struct must NOT be
    // reported; guards the `detect_dead_types` half of the enable condition.
    let mut config = crate::config::Config::default();
    config.duplicates.detect_dead_types = false;
    config.compile();
    let dry = run_dry(&config, "struct Unused { n: u8 }");
    assert!(
        dry.dead_types.is_empty(),
        "no dead-type warnings when detect_dead_types is off, got {:?}",
        dry.dead_types
    );
}

#[test]
fn dead_type_detection_runs_when_enabled() {
    // The counterpart: with the default config the same input DOES report, so
    // the test above pins the flag rather than an empty fixture.
    let mut config = crate::config::Config::default();
    config.compile();
    let dry = run_dry(&config, "struct Unused { n: u8 }");
    assert_eq!(dry.dead_types.len(), 1, "got {:?}", dry.dead_types);
}

#[test]
fn dead_type_detection_skipped_when_duplicates_disabled() {
    // The DRY dimension's own switch must also silence it.
    let mut config = crate::config::Config::default();
    config.duplicates.enabled = false;
    config.compile();
    let dry = run_dry(&config, "struct Unused { n: u8 }");
    assert!(dry.dead_types.is_empty(), "got {:?}", dry.dead_types);
}

#[test]
fn wildcard_detection_skipped_when_duplicates_disabled() {
    // `detect_wildcard_imports` stays on but `duplicates.enabled = false`, so the
    // wildcard closure's `if !c.duplicates.enabled` must short-circuit to empty;
    // guards that `!`.
    let mut config = crate::config::Config::default();
    config.duplicates.enabled = false;
    config.compile();
    let dry = run_dry(&config, "use crate::foo::*;");
    assert!(
        dry.wildcard_warnings.is_empty(),
        "no wildcard warnings when duplicates is disabled, got {:?}",
        dry.wildcard_warnings
    );
}
