/// Severity threshold: violations with more than this many total locations are HIGH.
pub const SEVERITY_HIGH_THRESHOLD: usize = 5;
/// Severity threshold: violations with more than this many total locations are MEDIUM.
pub const SEVERITY_MEDIUM_THRESHOLD: usize = 2;
/// Multiplier for converting score ratio (0.0–1.0) to percentage (0–100).
pub const PERCENTAGE_MULTIPLIER: f64 = 100.0;

/// Classification of a function according to IOSP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classification {
    /// Pure orchestration: calls other functions but contains no own logic.
    Integration,
    /// Pure logic: contains control flow / computation but no calls to own functions.
    Operation,
    /// Violates IOSP: mixes both logic and own function calls.
    Violation {
        has_logic: bool,
        has_own_calls: bool,
        logic_locations: Vec<LogicOccurrence>,
        call_locations: Vec<CallOccurrence>,
    },
    /// Trivial function (e.g. single return, getter, delegation) — not worth flagging.
    Trivial,
}

pub use crate::domain::Severity;

/// A location where deep nesting contributes to complexity.
#[derive(Debug, Clone, Default, PartialEq, Eq, derive_more::Display)]
#[display("{construct} at nesting {nesting_depth} (line {line})")]
pub struct ComplexityHotspot {
    pub line: usize,
    pub nesting_depth: usize,
    pub construct: String,
}

/// A magic number literal found in non-const context.
#[derive(Debug, Clone, Default, PartialEq, Eq, derive_more::Display)]
#[display("{value} (line {line})")]
pub struct MagicNumberOccurrence {
    pub line: usize,
    pub value: String,
}

/// Complexity metrics for a function.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComplexityMetrics {
    pub logic_count: usize,
    pub call_count: usize,
    pub max_nesting: usize,
    pub cognitive_complexity: usize,
    pub cyclomatic_complexity: usize,
    pub hotspots: Vec<ComplexityHotspot>,
    pub magic_numbers: Vec<MagicNumberOccurrence>,
    /// Number of lines in the function body (brace-to-brace).
    pub function_lines: usize,
    /// Number of unsafe blocks in the function body.
    pub unsafe_blocks: usize,
    /// Number of `.unwrap()` calls.
    pub unwrap_count: usize,
    /// Number of `.expect()` calls.
    pub expect_count: usize,
    /// Number of `panic!` / `unreachable!` macro invocations.
    pub panic_count: usize,
    /// Number of `todo!` macro invocations.
    pub todo_count: usize,
    /// All logic occurrences with their line numbers (for TQ-005 coverage analysis).
    pub logic_occurrences: Vec<LogicOccurrence>,
}

#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
#[display("{kind} (line {line})")]
pub struct LogicOccurrence {
    pub kind: String, // "if", "match", "for", "while", "loop", "arithmetic", "boolean_op", "?"
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
#[display("{name} (line {line})")]
pub struct CallOccurrence {
    pub name: String,
    pub line: usize,
}

/// Compute severity from a classification's violation data.
/// Operation: match on classification with arithmetic logic.
pub fn compute_severity(classification: &Classification) -> Option<Severity> {
    if let Classification::Violation {
        logic_locations,
        call_locations,
        ..
    } = classification
    {
        let total = logic_locations.len() + call_locations.len();
        if total > SEVERITY_HIGH_THRESHOLD {
            Some(Severity::High)
        } else if total > SEVERITY_MEDIUM_THRESHOLD {
            Some(Severity::Medium)
        } else {
            Some(Severity::Low)
        }
    } else {
        None
    }
}

// ── Effort score constants ──────────────────────────────────────

/// Weight for logic occurrences in effort score.
pub const EFFORT_LOGIC_WEIGHT: f64 = 1.0;
/// Weight for call occurrences in effort score.
pub const EFFORT_CALL_WEIGHT: f64 = 1.5;
/// Weight for nesting depth in effort score.
pub const EFFORT_NESTING_WEIGHT: f64 = 2.0;

/// Result of analyzing a single function.
#[derive(Debug, Clone)]
pub struct FunctionAnalysis {
    pub name: String,
    pub file: String,
    pub line: usize,
    pub classification: Classification,
    /// Optional: the parent impl type (e.g. "MyStruct")
    pub parent_type: Option<String>,
    /// Whether this function was suppressed via `// iosp:allow`.
    pub suppressed: bool,
    /// Complexity metrics (available when function is non-trivial).
    pub complexity: Option<ComplexityMetrics>,
    /// Pre-computed qualified name (e.g. "MyStruct::method" or "free_fn").
    pub qualified_name: String,
    /// Pre-computed severity (only Some for Violations).
    pub severity: Option<Severity>,
    /// Whether cognitive complexity exceeds threshold (set by pipeline).
    pub cognitive_warning: bool,
    /// Whether cyclomatic complexity exceeds threshold (set by pipeline).
    pub cyclomatic_warning: bool,
    /// Whether nesting depth exceeds threshold (set by pipeline).
    pub nesting_depth_warning: bool,
    /// Whether function length exceeds threshold (set by pipeline).
    pub function_length_warning: bool,
    /// Whether the function contains unsafe blocks (set by pipeline).
    pub unsafe_warning: bool,
    /// Whether the function contains error-handling issues (set by pipeline).
    pub error_handling_warning: bool,
    /// Whether complexity warnings are suppressed via `// qual:allow(complexity)`.
    pub complexity_suppressed: bool,
    /// Deduped own-call target names (for module cohesion SRP analysis).
    pub own_calls: Vec<String>,
    /// Number of non-self parameters (for SRP-004 parameter count check).
    pub parameter_count: usize,
    /// Whether this function is inside a trait impl (trait impls can't change their signature).
    pub is_trait_impl: bool,
    /// Whether this function is a test (`#[test]` or inside `#[cfg(test)]`).
    pub is_test: bool,
    /// Estimated refactoring effort for violations (None for non-violations).
    pub effort_score: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
