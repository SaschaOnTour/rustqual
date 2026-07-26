use std::process::Command;

fn cargo_bin() -> Command {
    let mut cmd = Command::new("cargo");
    cmd.arg("run")
        .arg("--")
        .current_dir(env!("CARGO_MANIFEST_DIR"));
    cmd
}

#[test]
fn test_self_analysis_no_violations() {
    // Must run with "." (not "src/") as the analysis root. Architecture
    // rule globs (e.g. `src/adapters/**`) match against paths relative
    // to the analysis root — running with "src/" would strip the
    // prefix and silently disable every architecture check.
    let output = cargo_bin().args(["."]).output().expect("Failed to execute");
    assert!(
        output.status.success(),
        "Self-analysis should have 0 violations.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn test_sample_file_expected_results() {
    let output = cargo_bin()
        .args(["examples/sample.rs", "--json", "--no-fail"])
        .output()
        .expect("Failed to execute");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Invalid JSON output");

    let functions = json["functions"].as_array().unwrap();

    // Check expected classifications
    let find_fn = |name: &str| -> String {
        functions
            .iter()
            .find(|f| f["name"] == name)
            .unwrap_or_else(|| panic!("Function '{}' not found in output", name))["classification"]
            .as_str()
            .unwrap()
            .to_string()
    };

    assert_eq!(find_fn("calculate_discount"), "operation");
    assert_eq!(find_fn("validate_email"), "operation");
    assert_eq!(find_fn("process_order"), "integration");
    assert_eq!(find_fn("handle_user_registration"), "integration");
    assert_eq!(find_fn("process_payment"), "violation");
    assert_eq!(find_fn("generate_report"), "violation");
    assert_eq!(find_fn("get_name"), "trivial");
}

#[test]
fn test_json_output_schema_and_complexity_values() {
    let output = cargo_bin()
        .args(["examples/sample.rs", "--json", "--no-fail"])
        .output()
        .expect("Failed to execute");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("JSON output must be valid");

    assert!(json.get("summary").is_some(), "Must have 'summary' key");
    assert!(json.get("functions").is_some(), "Must have 'functions' key");

    let summary = &json["summary"];
    assert!(summary["total"].as_u64().unwrap() > 0);

    // Value-level guard against A21 (smoke-tests mask projection drops).
    // sample.rs has at least one Operation with non-zero logic_count;
    // assert the field survives the projection + reporter round-trip.
    let funcs = json["functions"].as_array().expect("functions array");
    let with_logic = funcs
        .iter()
        .filter_map(|f| f.get("complexity"))
        .filter_map(|c| c.get("logic_count").and_then(|v| v.as_u64()))
        .max()
        .unwrap_or(0);
    assert!(
        with_logic > 0,
        "at least one function's complexity.logic_count must be non-zero on sample.rs"
    );
}

#[test]
fn test_verbose_shows_all() {
    let output = cargo_bin()
        .args(["examples/sample.rs", "--verbose", "--no-fail"])
        .output()
        .expect("Failed to execute");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verbose mode should show all classification types
    assert!(
        stdout.contains("INTEGRATION"),
        "Verbose should show integrations"
    );
    assert!(
        stdout.contains("OPERATION"),
        "Verbose should show operations"
    );
    assert!(stdout.contains("TRIVIAL"), "Verbose should show trivials");
    assert!(
        stdout.contains("VIOLATION"),
        "Verbose should show violations"
    );
}

/// DRY-006 end to end: source → detector → JSON envelope → exit code. Every
/// unit test below this level works on already-parsed fixtures, so nothing else
/// proves the finding actually travels the pipeline or fails the run.
#[test]
fn test_dead_type_reaches_the_json_envelope_and_fails_the_run() {
    let dir = tempfile::Builder::new()
        .prefix("test_dead_type_")
        .tempdir()
        .expect("temp dir");
    std::fs::create_dir_all(dir.path().join("src")).expect("src dir");
    std::fs::write(
        dir.path().join("src/lib.rs"),
        "pub struct Orphan { pub n: u8 }\npub struct Alive;\npub fn make() -> Alive { Alive }\n",
    )
    .expect("write lib.rs");

    let failing = cargo_bin()
        .args([dir.path().to_str().expect("utf-8 path")])
        .output()
        .expect("Failed to execute");
    assert!(
        !failing.status.success(),
        "a dead type must fail the run by default"
    );

    let output = cargo_bin()
        .args([
            dir.path().to_str().expect("utf-8 path"),
            "--json",
            "--no-fail",
        ])
        .output()
        .expect("Failed to execute");
    assert!(
        output.status.success(),
        "--no-fail must exit zero, or the parse below fails for the wrong reason: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Invalid JSON output");

    let dead_types = json["dead_types"].as_array().expect("dead_types array");
    assert_eq!(dead_types.len(), 1, "only `Orphan` is dead: {stdout}");
    assert_eq!(dead_types[0]["name"], "Orphan");
    assert_eq!(dead_types[0]["item"], "struct");
    assert_eq!(
        json["summary"]["dead_type_warnings"], 1,
        "the summary count must agree with the array"
    );
}
