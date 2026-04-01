//! IOSP Showcase: Before/After comparison demonstrating the
//! Integration Operation Segregation Principle via a UserService example.

use std::io::Write;
use std::process::Command;

fn cargo_bin() -> Command {
    let mut cmd = Command::new("cargo");
    cmd.arg("run")
        .arg("--")
        .current_dir(env!("CARGO_MANIFEST_DIR"));
    cmd
}

/// Helper: write source to a temp file, analyze with --json, return parsed JSON.
fn analyze_source(source: &str) -> serde_json::Value {
    let mut tmp = tempfile::Builder::new()
        .prefix("iosp_showcase_")
        .suffix(".rs")
        .tempfile()
        .expect("Failed to create temp file");
    tmp.write_all(source.as_bytes())
        .expect("Failed to write temp file");
    tmp.flush().unwrap();

    let output = cargo_bin()
        .args([tmp.path().to_str().unwrap(), "--json", "--no-fail"])
        .output()
        .expect("Failed to execute");

    assert!(
        output.status.success(),
        "Analysis failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).expect("Invalid JSON output")
}

/// Helper: find a function's classification in the JSON output.
fn find_classification(json: &serde_json::Value, fn_name: &str) -> String {
    json["functions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == fn_name)
        .unwrap_or_else(|| panic!("Function '{fn_name}' not found in output"))["classification"]
        .as_str()
        .unwrap()
        .to_string()
}

const BEFORE_IOSP: &str = r#"
struct UserService;
struct User { name: String, email: String, age: u32 }

fn create_user(name: &str, email: &str, age: u32) -> User {
    User { name: name.to_string(), email: email.to_string(), age }
}
fn save_to_database(_user: &User) {}
fn send_welcome_email(_user: &User) {}
fn charge_payment(_user: &User, _amount: f64) {}
fn send_receipt(_user: &User, _amount: f64) {}

impl UserService {
    // VIOLATION: mixes validation logic with calls
    fn register_user(name: &str, email: &str, age: u32) -> Result<User, String> {
        if name.is_empty() {
            return Err("Name required".into());
        }
        if !email.contains('@') {
            return Err("Invalid email".into());
        }
        if age < 18 {
            return Err("Must be 18+".into());
        }
        let user = create_user(name, email, age);
        save_to_database(&user);
        send_welcome_email(&user);
        Ok(user)
    }

    // VIOLATION: mixes calculation with calls
    fn process_order(user: &User, amount: f64) -> Result<f64, String> {
        let discount = if amount > 100.0 { 0.1 } else { 0.0 };
        let final_amount = amount * (1.0 - discount);
        charge_payment(user, final_amount);
        send_receipt(user, final_amount);
        Ok(final_amount)
    }
}
"#;

const AFTER_IOSP: &str = r#"
struct UserService;
struct User { name: String, email: String, age: u32 }

fn create_user(name: &str, email: &str, age: u32) -> User {
    User { name: name.to_string(), email: email.to_string(), age }
}
fn save_to_database(_user: &User) {}
fn send_welcome_email(_user: &User) {}
fn charge_payment(_user: &User, _amount: f64) {}
fn send_receipt(_user: &User, _amount: f64) {}

impl UserService {
    // INTEGRATION: pure delegation, no own logic
    fn register_user(name: &str, email: &str, age: u32) -> Result<User, String> {
        validate_registration(name, email, age)?;
        let user = create_user(name, email, age);
        save_to_database(&user);
        send_welcome_email(&user);
        Ok(user)
    }

    // OPERATION: pure validation logic, no own calls
    fn validate_registration(name: &str, email: &str, age: u32) -> Result<(), String> {
        if name.is_empty() {
            return Err("Name required".into());
        }
        if !email.contains('@') {
            return Err("Invalid email".into());
        }
        if age < 18 {
            return Err("Must be 18+".into());
        }
        Ok(())
    }

    // OPERATION: pure calculation
    fn calculate_final_amount(amount: f64) -> f64 {
        let discount = if amount > 100.0 { 0.1 } else { 0.0 };
        amount * (1.0 - discount)
    }

    // INTEGRATION: pure delegation
    fn process_order(user: &User, amount: f64) -> Result<f64, String> {
        let final_amount = calculate_final_amount(amount);
        charge_payment(user, final_amount);
        send_receipt(user, final_amount);
        Ok(final_amount)
    }
}
"#;

#[test]
fn test_before_iosp_has_violations() {
    let json = analyze_source(BEFORE_IOSP);

    assert_eq!(
        find_classification(&json, "register_user"),
        "violation",
        "register_user should be a Violation (mixes logic + calls)"
    );
    assert_eq!(
        find_classification(&json, "process_order"),
        "violation",
        "process_order should be a Violation (mixes logic + calls)"
    );
}

#[test]
fn test_after_iosp_no_violations() {
    let json = analyze_source(AFTER_IOSP);

    let violations: Vec<_> = json["functions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["classification"] == "violation")
        .collect();
    assert!(
        violations.is_empty(),
        "After IOSP refactoring there should be 0 violations, found: {violations:?}"
    );

    assert_eq!(find_classification(&json, "register_user"), "integration");
    assert_eq!(
        find_classification(&json, "validate_registration"),
        "operation"
    );
    assert_eq!(
        find_classification(&json, "calculate_final_amount"),
        "operation"
    );
    assert_eq!(find_classification(&json, "process_order"), "integration");
}

#[test]
fn test_iosp_refactoring_improves_score() {
    let before_json = analyze_source(BEFORE_IOSP);
    let after_json = analyze_source(AFTER_IOSP);

    let before_score = before_json["summary"]["iosp_score"].as_f64().unwrap();
    let after_score = after_json["summary"]["iosp_score"].as_f64().unwrap();

    assert!(
        before_score < 1.0,
        "Before IOSP: score should be < 1.0, got {before_score}"
    );
    assert!(
        (after_score - 1.0).abs() < f64::EPSILON,
        "After IOSP: score should be 1.0, got {after_score}"
    );
    assert!(
        after_score > before_score,
        "After should have higher score ({after_score}) than before ({before_score})"
    );
}
