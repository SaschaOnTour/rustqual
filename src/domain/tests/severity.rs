use crate::domain::Severity;

#[test]
fn severity_variants_are_comparable_for_equality() {
    assert_eq!(Severity::Low, Severity::Low);
    assert_ne!(Severity::Low, Severity::High);
    assert_ne!(Severity::Medium, Severity::High);
}

#[test]
fn severity_serializes_as_lowercase() {
    let low_json = serde_json::to_string(&Severity::Low).unwrap();
    let med_json = serde_json::to_string(&Severity::Medium).unwrap();
    let high_json = serde_json::to_string(&Severity::High).unwrap();
    assert_eq!(low_json, "\"low\"");
    assert_eq!(med_json, "\"medium\"");
    assert_eq!(high_json, "\"high\"");
}
