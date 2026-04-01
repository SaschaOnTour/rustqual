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
    let output = cargo_bin()
        .args(["src/"])
        .output()
        .expect("Failed to execute");
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
fn test_json_output_parseable() {
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
