/// Build the SARIF rules array with all known rule definitions.
/// Operation: static data construction, no own calls.
pub(super) fn sarif_rules() -> Vec<serde_json::Value> {
    vec![
        rule(
            "iosp/violation",
            "Function violates the Integration Operation Segregation Principle",
        ),
        rule("CX-001", "Cognitive complexity exceeds threshold"),
        rule("CX-002", "Cyclomatic complexity exceeds threshold"),
        rule("CX-003", "Magic number literal in non-const context"),
        rule("CP-001", "Circular module dependency"),
        rule("DRY-001", "Duplicate function detected"),
        rule("DRY-002", "Dead code detected"),
        rule("DRY-003", "Duplicate code fragment"),
        rule(
            "SRP-001",
            "Struct may violate Single Responsibility Principle",
        ),
        rule("SRP-002", "Module has excessive production code length"),
        rule(
            "SRP-003",
            "Function has too many parameters — reduce parameter count",
        ),
        rule("CX-004", "Function length exceeds threshold"),
        rule("CX-005", "Nesting depth exceeds threshold"),
        rule("CX-006", "Unsafe block detected"),
        rule("A20", "Error handling issue (unwrap/expect/panic/todo)"),
        rule("DRY-004", "Wildcard import (use module::*)"),
        rule("CP-002", "Stable Dependencies Principle violation"),
        rule("TQ-001", "Test function has no assertions"),
        rule(
            "TQ-002",
            "Test function does not call any production function",
        ),
        rule("TQ-003", "Production function is untested"),
        rule("TQ-004", "Production function has no coverage"),
        rule("TQ-005", "Untested logic branches (uncovered lines)"),
        rule("BTC", "Broken trait contract: all methods are stubs"),
        rule("SLM", "Selfless method: takes self but never references it"),
        rule(
            "NMS",
            "Needless &mut self: takes &mut self but never mutates",
        ),
        rule(
            "OI",
            "Orphaned impl: impl block in different file than type",
        ),
        rule(
            "SIT",
            "Single-impl trait: non-pub trait with exactly one implementation",
        ),
        rule("DEH", "Downcast escape hatch: use of Any::downcast"),
        rule("IET", "Inconsistent error types in module"),
        rule("DRY-005", "Repeated match pattern across functions"),
        rule("BP-001", "Trivial From implementation (derivable)"),
        rule("BP-002", "Trivial Display implementation (derivable)"),
        rule(
            "BP-003",
            "Trivial getter/setter (consider field visibility)",
        ),
        rule("BP-004", "Builder pattern (consider derive macro)"),
        rule("BP-005", "Manual Default implementation (derivable)"),
        rule("BP-006", "Repetitive match mapping"),
        rule("BP-007", "Error enum boilerplate (consider thiserror)"),
        rule("BP-008", "Clone-heavy conversion"),
        rule("BP-009", "Struct update boilerplate"),
        rule("BP-010", "Format string repetition"),
        rule("SUP-001", "Suppression ratio exceeds configured maximum"),
    ]
}

/// Build a single SARIF rule entry.
/// Operation: JSON construction.
fn rule(id: &str, description: &str) -> serde_json::Value {
    let mut entry = serde_json::json!({
        "id": id,
        "shortDescription": { "text": description }
    });
    if id == "iosp/violation" {
        entry.as_object_mut().expect("rule is object").insert(
            "helpUri".to_string(),
            serde_json::json!("https://flow-design.info/"),
        );
    }
    entry
}
