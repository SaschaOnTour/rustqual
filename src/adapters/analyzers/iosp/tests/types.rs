use crate::adapters::analyzers::iosp::types::*;

#[test]
fn test_logic_occurrence_display() {
    let lo = LogicOccurrence {
        kind: "if".to_string(),
        line: 42,
    };
    assert_eq!(lo.to_string(), "if (line 42)");
}

#[test]
fn test_call_occurrence_display() {
    let co = CallOccurrence {
        name: "helper".to_string(),
        line: 10,
    };
    assert_eq!(co.to_string(), "helper (line 10)");
}

#[test]
fn test_complexity_hotspot_display() {
    let h = ComplexityHotspot {
        line: 15,
        nesting_depth: 3,
        construct: "if".to_string(),
    };
    assert_eq!(h.to_string(), "if at nesting 3 (line 15)");
}

#[test]
fn test_magic_number_occurrence_display() {
    let m = MagicNumberOccurrence {
        line: 7,
        value: "42".to_string(),
    };
    assert_eq!(m.to_string(), "42 (line 7)");
}

#[test]
fn test_complexity_metrics_default() {
    let m = ComplexityMetrics::default();
    assert_eq!(m.logic_count, 0);
    assert_eq!(m.call_count, 0);
    assert_eq!(m.max_nesting, 0);
    assert_eq!(m.cognitive_complexity, 0);
    assert_eq!(m.cyclomatic_complexity, 0);
    assert!(m.hotspots.is_empty());
    assert!(m.magic_numbers.is_empty());
}

#[test]
fn test_compute_severity_low() {
    let c = Classification::Violation {
        has_logic: true,
        has_own_calls: true,
        logic_locations: vec![LogicOccurrence {
            kind: "if".into(),
            line: 1,
        }],
        call_locations: vec![CallOccurrence {
            name: "f".into(),
            line: 2,
        }],
    };
    assert_eq!(compute_severity(&c), Some(Severity::Low));
}

#[test]
fn test_compute_severity_medium() {
    let c = Classification::Violation {
        has_logic: true,
        has_own_calls: true,
        logic_locations: vec![
            LogicOccurrence {
                kind: "if".into(),
                line: 1,
            },
            LogicOccurrence {
                kind: "match".into(),
                line: 2,
            },
        ],
        call_locations: vec![CallOccurrence {
            name: "f".into(),
            line: 3,
        }],
    };
    assert_eq!(compute_severity(&c), Some(Severity::Medium));
}

#[test]
fn test_compute_severity_high() {
    let c = Classification::Violation {
        has_logic: true,
        has_own_calls: true,
        logic_locations: vec![
            LogicOccurrence {
                kind: "if".into(),
                line: 1,
            },
            LogicOccurrence {
                kind: "match".into(),
                line: 2,
            },
            LogicOccurrence {
                kind: "for".into(),
                line: 3,
            },
        ],
        call_locations: vec![
            CallOccurrence {
                name: "a".into(),
                line: 4,
            },
            CallOccurrence {
                name: "b".into(),
                line: 5,
            },
            CallOccurrence {
                name: "c".into(),
                line: 6,
            },
        ],
    };
    assert_eq!(compute_severity(&c), Some(Severity::High));
}

#[test]
fn test_compute_severity_none_for_non_violation() {
    assert_eq!(compute_severity(&Classification::Integration), None);
    assert_eq!(compute_severity(&Classification::Operation), None);
    assert_eq!(compute_severity(&Classification::Trivial), None);
}
