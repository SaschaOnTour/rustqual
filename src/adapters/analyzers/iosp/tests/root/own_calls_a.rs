use super::*;

#[test]
fn test_method_on_non_project_type_not_own_call() {
    // Cache defines .clear(), but reset_name calls .clear() on a String parameter.
    // String::clear is NOT an own call — different type.
    let code = r#"
        struct Cache { data: Vec<i32> }
        impl Cache {
            fn clear(&mut self) {
                self.data = Vec::new();
            }
        }
        fn reset_name(name: &mut String) {
            if name.is_empty() { return; }
            name.clear();
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|f| f.name == "reset_name").unwrap();
    assert!(
        matches!(f.classification, Classification::Operation),
        "name.clear() is String::clear, not Cache::clear — should be Operation, got {:?}",
        f.classification
    );
}

#[test]
fn test_self_method_call_is_own_call() {
    // self.process() IS an own call — it's on the same type
    let code = r#"
        struct Engine;
        impl Engine {
            fn process(&self) -> i32 { 42 }
            fn run(&self) {
                self.process();
            }
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|f| f.name == "run").unwrap();
    assert!(
        matches!(f.classification, Classification::Integration),
        "self.process() is own call — should be Integration, got {:?}",
        f.classification
    );
}

#[test]
fn test_method_on_param_project_type_is_own_call() {
    // db.query() where db is a project type parameter — IS an own call
    let code = r#"
        struct Database;
        impl Database {
            fn query(&self) -> Vec<String> { vec![] }
        }
        fn fetch(db: &Database) {
            db.query();
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|f| f.name == "fetch").unwrap();
    assert!(
        matches!(f.classification, Classification::Integration),
        "db.query() on project type param — should be Integration, got {:?}",
        f.classification
    );
}

#[test]
fn test_method_name_collision_resolved_by_type() {
    // Both Formatter and Vec have "push". Formatter::push is own,
    // but v.push() on a Vec parameter should NOT be an own call.
    let code = r#"
        struct Formatter { parts: Vec<String> }
        impl Formatter {
            fn push(&mut self, s: String) {
                self.parts.push(s);
            }
        }
        fn collect_items(v: &mut Vec<String>) {
            if v.is_empty() { return; }
            v.push("done".to_string());
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|f| f.name == "collect_items").unwrap();
    assert!(
        matches!(f.classification, Classification::Operation),
        "v.push() on Vec param — not an own call, should be Operation, got {:?}",
        f.classification
    );
}

// ---------------------------------------------------------------
// Automatic leaf detection tests
// ---------------------------------------------------------------

#[test]
fn test_leaf_call_not_counted_as_own_call() {
    // get_config is a leaf (C=0, Operation).
    // cmd_quality calls get_config + has logic → should be Operation (leaf calls don't count).
    let code = r#"
        fn get_config() -> i32 {
            if true { 1 } else { 2 }
        }
        fn cmd_quality(clear: bool) -> i32 {
            let config = get_config();
            if clear { config + 1 } else { config }
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|f| f.name == "cmd_quality").unwrap();
    assert!(
        matches!(f.classification, Classification::Operation),
        "Calling a leaf (get_config) + logic should be Operation, got {:?}",
        f.classification
    );
}

#[test]
fn test_non_leaf_call_still_violation() {
    // bad_a and bad_b form a cycle — both are Violations that can't be reclassified.
    // caller has logic + calls bad_a → stays Violation.
    let code = r#"
        fn bad_a(x: bool) -> i32 {
            if x { bad_b(false) } else { 0 }
        }
        fn bad_b(x: bool) -> i32 {
            if x { bad_a(true) } else { 1 }
        }
        fn caller(x: bool) -> i32 {
            if x { bad_a(true) } else { 0 }
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|f| f.name == "caller").unwrap();
    assert!(
        matches!(f.classification, Classification::Violation { .. }),
        "Calling a non-leaf (orchestrator) + logic should be Violation, got {:?}",
        f.classification
    );
}

#[test]
fn test_multiple_leaf_calls_still_operation() {
    // Both helpers are leaves (C=0). Calling multiple leaves + logic → Operation.
    let code = r#"
        fn validate(s: &str) -> bool { s.len() > 3 }
        fn normalize(s: &str) -> String { s.to_lowercase() }
        fn process(input: &str) -> Option<String> {
            if validate(input) {
                Some(normalize(input))
            } else {
                None
            }
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|f| f.name == "process").unwrap();
    assert!(
        matches!(f.classification, Classification::Operation),
        "Calling only leaves + logic should be Operation, got {:?}",
        f.classification
    );
}
