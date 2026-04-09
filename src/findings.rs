/// The six analysis dimensions of rustqual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dimension {
    Iosp,
    Complexity,
    Dry,
    Srp,
    Coupling,
    Test,
}

impl Dimension {
    /// Parse a dimension name (case-insensitive).
    /// Operation: string matching logic.
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "iosp" => Some(Self::Iosp),
            "complexity" => Some(Self::Complexity),
            "dry" => Some(Self::Dry),
            "srp" => Some(Self::Srp),
            "coupling" => Some(Self::Coupling),
            "test" | "tq" => Some(Self::Test),
            _ => None,
        }
    }
}

impl std::fmt::Display for Dimension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Iosp => write!(f, "iosp"),
            Self::Complexity => write!(f, "complexity"),
            Self::Dry => write!(f, "dry"),
            Self::Srp => write!(f, "srp"),
            Self::Coupling => write!(f, "coupling"),
            Self::Test => write!(f, "test"),
        }
    }
}

/// A parsed suppression comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suppression {
    /// Line number where the suppression comment appears (1-based).
    pub line: usize,
    /// Which dimensions to suppress. Empty means suppress all.
    pub dimensions: Vec<Dimension>,
    /// Optional reason for the suppression.
    pub reason: Option<String>,
}

impl Suppression {
    /// Check if this suppression covers a given dimension.
    /// Operation: empty means all dimensions.
    pub fn covers(&self, dim: Dimension) -> bool {
        self.dimensions.is_empty() || self.dimensions.contains(&dim)
    }
}

/// Maximum number of lines between an annotation comment and the function/struct it applies to.
/// Allows stacking multiple annotations (e.g., `// qual:api` + `// qual:allow(iosp)`) and
/// accommodates `#[derive]` attributes between comment and definition.
pub const ANNOTATION_WINDOW: usize = 3;

/// Check if `target_line` is within the annotation window below `annotation_line`.
/// Operation: arithmetic comparison.
pub fn is_within_window(annotation_line: usize, target_line: usize) -> bool {
    annotation_line <= target_line && target_line - annotation_line <= ANNOTATION_WINDOW
}

/// Check if any line in a set is within the annotation window above `target_line`.
/// Operation: range iteration with set lookup.
pub fn has_annotation_in_window(
    lines: &std::collections::HashSet<usize>,
    target_line: usize,
) -> bool {
    (0..=ANNOTATION_WINDOW).any(|off| target_line >= off && lines.contains(&(target_line - off)))
}

/// Check if a trimmed line is a `// qual:api` marker.
/// Operation: string prefix check.
pub fn is_api_marker(trimmed: &str) -> bool {
    trimmed == "// qual:api" || trimmed.starts_with("// qual:api ")
}

/// Check if a trimmed line is a `// qual:allow(unsafe)` marker.
/// Operation: string check.
pub fn is_unsafe_allow_marker(trimmed: &str) -> bool {
    trimmed == "// qual:allow(unsafe)" || trimmed.starts_with("// qual:allow(unsafe) ")
}

/// Check if a trimmed line is a `// qual:recursive` marker.
/// Operation: string prefix check.
pub fn is_recursive_marker(trimmed: &str) -> bool {
    trimmed == "// qual:recursive" || trimmed.starts_with("// qual:recursive ")
}

/// Parse a `// qual:inverse(fn_name)` marker, returning the target function name.
/// Operation: string parsing logic.
pub fn parse_inverse_marker(trimmed: &str) -> Option<String> {
    trimmed
        .strip_prefix("// qual:inverse(")
        .and_then(|rest| rest.strip_suffix(')'))
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
}

/// Parse a suppression comment line into a Suppression struct.
/// Trivial: delegates to sub-parsers via closure chains.
pub fn parse_suppression(line_number: usize, trimmed: &str) -> Option<Suppression> {
    // qual:allow(unsafe) is a separate annotation, not a suppression
    if is_unsafe_allow_marker(trimmed) {
        return None;
    }
    trimmed
        .strip_prefix("// qual:allow")
        .map(|rest| parse_qual_allow(line_number, rest))
        .or_else(|| parse_iosp_legacy(line_number, trimmed))
}

/// Parse legacy `// iosp:allow` syntax.
/// Operation: string matching logic, no own calls.
fn parse_iosp_legacy(line_number: usize, trimmed: &str) -> Option<Suppression> {
    if trimmed == "// iosp:allow" || trimmed.starts_with("// iosp:allow ") {
        let reason = trimmed
            .strip_prefix("// iosp:allow ")
            .map(|s| s.to_string());
        Some(Suppression {
            line: line_number,
            dimensions: vec![Dimension::Iosp],
            reason,
        })
    } else {
        None
    }
}

/// Parse the part after "// qual:allow".
/// Operation: string parsing for dimensions and reason (no own calls;
/// extract_reason is called via closures for IOSP compliance).
fn parse_qual_allow(line_number: usize, rest: &str) -> Suppression {
    let rest = rest.trim();

    let (dimensions, reason_text) = if rest.is_empty() || !rest.starts_with('(') {
        (vec![], rest)
    } else {
        let close_paren = rest.find(')').unwrap_or(rest.len());
        let dims_str = &rest[1..close_paren];
        let dimensions: Vec<Dimension> = dims_str
            .split(',')
            .filter_map(|s| Dimension::from_str_opt(s.trim()))
            .collect();
        let after_parens = rest.get(close_paren + 1..).map(str::trim).unwrap_or("");
        (dimensions, after_parens)
    };

    let reason = (!reason_text.is_empty())
        .then(|| extract_reason(reason_text))
        .flatten();

    Suppression {
        line: line_number,
        dimensions,
        reason,
    }
}

/// Extract a reason from text like `reason: "some text"` or bare text.
/// Operation: string parsing logic, no own calls.
fn extract_reason(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Some(rest) = text.strip_prefix("reason:") {
        let rest = rest.trim();
        if rest.starts_with('"') && rest.ends_with('"') && rest.len() > 1 {
            return Some(rest[1..rest.len() - 1].to_string());
        }
        return Some(rest.to_string());
    }
    Some(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dimension_display() {
        assert_eq!(Dimension::Iosp.to_string(), "iosp");
        assert_eq!(Dimension::Complexity.to_string(), "complexity");
        assert_eq!(Dimension::Dry.to_string(), "dry");
        assert_eq!(Dimension::Srp.to_string(), "srp");
        assert_eq!(Dimension::Coupling.to_string(), "coupling");
        assert_eq!(Dimension::Test.to_string(), "test");
    }

    #[test]
    fn test_dimension_from_str() {
        assert_eq!(Dimension::from_str_opt("iosp"), Some(Dimension::Iosp));
        assert_eq!(
            Dimension::from_str_opt("COMPLEXITY"),
            Some(Dimension::Complexity)
        );
        assert_eq!(Dimension::from_str_opt("DRY"), Some(Dimension::Dry));
        assert_eq!(Dimension::from_str_opt("test"), Some(Dimension::Test));
        assert_eq!(Dimension::from_str_opt("tq"), Some(Dimension::Test));
        assert_eq!(Dimension::from_str_opt("unknown"), None);
    }

    #[test]
    fn test_suppression_covers_all() {
        let s = Suppression {
            line: 1,
            dimensions: vec![],
            reason: None,
        };
        assert!(s.covers(Dimension::Iosp));
        assert!(s.covers(Dimension::Complexity));
        assert!(s.covers(Dimension::Dry));
    }

    #[test]
    fn test_suppression_covers_specific() {
        let s = Suppression {
            line: 1,
            dimensions: vec![Dimension::Iosp],
            reason: None,
        };
        assert!(s.covers(Dimension::Iosp));
        assert!(!s.covers(Dimension::Complexity));
    }

    #[test]
    fn test_parse_qual_allow_all() {
        let s = parse_suppression(5, "// qual:allow").unwrap();
        assert_eq!(s.line, 5);
        assert!(s.dimensions.is_empty());
        assert!(s.reason.is_none());
    }

    #[test]
    fn test_parse_qual_allow_iosp() {
        let s = parse_suppression(3, "// qual:allow(iosp)").unwrap();
        assert_eq!(s.dimensions, vec![Dimension::Iosp]);
        assert!(s.reason.is_none());
    }

    #[test]
    fn test_parse_qual_allow_multiple_dims() {
        let s = parse_suppression(1, "// qual:allow(iosp, complexity)").unwrap();
        assert_eq!(s.dimensions, vec![Dimension::Iosp, Dimension::Complexity]);
    }

    #[test]
    fn test_parse_qual_allow_with_reason() {
        let s =
            parse_suppression(1, "// qual:allow(iosp) reason: \"syn visitor pattern\"").unwrap();
        assert_eq!(s.dimensions, vec![Dimension::Iosp]);
        assert_eq!(s.reason.as_deref(), Some("syn visitor pattern"));
    }

    #[test]
    fn test_parse_old_iosp_allow_still_works() {
        let s = parse_suppression(10, "// iosp:allow").unwrap();
        assert_eq!(s.line, 10);
        assert_eq!(s.dimensions, vec![Dimension::Iosp]);
        assert!(s.reason.is_none());
    }

    #[test]
    fn test_parse_old_iosp_allow_with_reason() {
        let s = parse_suppression(1, "// iosp:allow justified reason").unwrap();
        assert_eq!(s.dimensions, vec![Dimension::Iosp]);
        assert_eq!(s.reason.as_deref(), Some("justified reason"));
    }

    #[test]
    fn test_parse_no_match() {
        assert!(parse_suppression(1, "// normal comment").is_none());
        assert!(parse_suppression(1, "let x = 42;").is_none());
    }

    // ── API marker tests ─────────────────────────────────────────

    #[test]
    fn test_api_marker_exact() {
        assert!(is_api_marker("// qual:api"));
    }

    #[test]
    fn test_api_marker_with_trailing_text() {
        assert!(is_api_marker("// qual:api public interface"));
    }

    #[test]
    fn test_api_marker_not_suppression() {
        assert!(!is_api_marker("// qual:allow(dry)"));
    }

    #[test]
    fn test_api_marker_not_regular_comment() {
        assert!(!is_api_marker("// normal comment"));
    }

    #[test]
    fn test_api_marker_not_counted_as_suppression() {
        assert!(parse_suppression(1, "// qual:api").is_none());
    }

    #[test]
    fn test_inverse_marker_parsed() {
        assert_eq!(
            parse_inverse_marker("// qual:inverse(parse)"),
            Some("parse".to_string())
        );
    }

    #[test]
    fn test_inverse_marker_with_spaces() {
        assert_eq!(
            parse_inverse_marker("// qual:inverse( as_str )"),
            Some("as_str".to_string())
        );
    }

    #[test]
    fn test_inverse_marker_empty_rejected() {
        assert_eq!(parse_inverse_marker("// qual:inverse()"), None);
    }

    #[test]
    fn test_inverse_marker_not_suppression() {
        assert!(parse_suppression(1, "// qual:inverse(parse)").is_none());
    }
}
