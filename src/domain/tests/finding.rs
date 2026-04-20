use crate::domain::finding::Finding;
use crate::domain::{Dimension, Severity};

#[test]
fn default_finding_is_unsuppressed_and_zero_position() {
    let f = Finding::default();
    assert!(f.file.is_empty());
    assert_eq!(f.line, 0);
    assert_eq!(f.column, 0);
    assert_eq!(f.severity, Severity::Medium);
    assert!(!f.suppressed);
}

#[test]
fn struct_literal_construction_is_ergonomic() {
    let f = Finding {
        file: "src/foo.rs".to_string(),
        line: 1,
        dimension: Dimension::Iosp,
        rule_id: "iosp/VIOLATION".to_string(),
        message: "logic + calls".to_string(),
        severity: Severity::High,
        ..Default::default()
    };
    assert_eq!(f.file, "src/foo.rs");
    assert_eq!(f.rule_id, "iosp/VIOLATION");
    assert_eq!(f.severity, Severity::High);
}

#[test]
fn project_wide_finding_allowed() {
    let f = Finding {
        dimension: Dimension::Coupling,
        rule_id: "coupling/CYCLE".to_string(),
        message: "cycle: a > b > c".to_string(),
        severity: Severity::Medium,
        ..Default::default()
    };
    assert!(f.file.is_empty());
    assert_eq!(f.line, 0);
}
