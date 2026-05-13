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
