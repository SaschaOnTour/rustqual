// Re-export Domain types so existing `crate::findings::{Dimension, Suppression}`
// call sites keep working. The canonical location is `crate::domain`;
// subsequent phases will migrate call sites to import from there directly.
pub use crate::domain::{Dimension, Suppression};

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

/// Check if a trimmed line is a `// qual:test_helper` marker.
/// The annotation narrowly suppresses DRY-002 (`testonly` dead code)
/// and TQ-003 (untested) on a function that is only called from test
/// code — without silencing complexity, SRP, coupling, or DRY
/// duplicate checks the way `ignore_functions` would. It does not
/// count against `max_suppression_ratio`.
/// Operation: string prefix check.
pub fn is_test_helper_marker(trimmed: &str) -> bool {
    trimmed == "// qual:test_helper" || trimmed.starts_with("// qual:test_helper ")
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
    // qual:allow(unsafe) and qual:test_helper are separate annotations,
    // not suppressions — they must not count against max_suppression_ratio.
    if is_unsafe_allow_marker(trimmed) || is_test_helper_marker(trimmed) {
        return None;
    }
    trimmed
        .strip_prefix("// qual:allow")
        .and_then(|rest| parse_qual_allow(line_number, rest))
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

// qual:api
/// Detect a `// qual:allow(...)` marker whose parens are malformed
/// (missing close-paren, e.g. `// qual:allow(iosp`) OR contain text
/// but no recognized dimension name (typo: `srp_params`, removed
/// dim, stray text). Returns the offending spec for orphan-finding
/// text. Bare `// qual:allow`, `// qual:allow()`, and the
/// special-purpose `// qual:allow(unsafe)` form are NOT flagged —
/// the first two carry no intent, the third is its own annotation
/// handled by `is_unsafe_allow_marker`. Must agree with
/// `parse_qual_allow`'s reject path so every malformed marker
/// either suppresses or surfaces as orphan, never both, never
/// silently dropped.
pub fn detect_invalid_qual_allow(trimmed: &str) -> Option<String> {
    if is_unsafe_allow_marker(trimmed) {
        return None;
    }
    let rest = trimmed.strip_prefix("// qual:allow")?.trim_start();
    if !rest.starts_with('(') {
        return None;
    }
    let (dims_str, malformed_parens) = match rest.find(')') {
        Some(close_paren) => (rest[1..close_paren].trim().to_string(), false),
        // Missing close paren: treat the whole tail (after `(`) as
        // the bad spec so the orphan finding is visible.
        None => (rest[1..].trim().to_string(), true),
    };
    if dims_str.is_empty() {
        return None;
    }
    if malformed_parens {
        // Structural malformation — surface even if the tail happens
        // to spell a valid dim. Mirrors the parser's reject path so a
        // user typing `// qual:allow(iosp` (no `)`) doesn't get
        // silently zero suppression and zero orphan.
        return Some(dims_str);
    }
    let any_recognized = dims_str
        .split(',')
        .any(|s| Dimension::from_str_opt(s.trim()).is_some());
    if any_recognized {
        return None;
    }
    Some(dims_str)
}

/// Parse the part after "// qual:allow".
/// Returns `None` for any form that produces an empty dimensions list:
/// bare allow, empty parens, all-args unrecognized, or unclosed parens
/// (missing the closing `)`) — none of those can act as suppressions.
/// Typos like `// qual:allow(srp_params)` and unclosed forms like
/// `// qual:allow(srp_params` are also surfaced as invalid markers
/// via `detect_invalid_qual_allow` so the author still sees a
/// stale-suppression finding.
/// Operation: string parsing for dimensions and reason (no own calls;
/// extract_reason is called via closures for IOSP compliance).
fn parse_qual_allow(line_number: usize, rest: &str) -> Option<Suppression> {
    let rest = rest.trim();

    let (dimensions, reason_text) = if rest.is_empty() || !rest.starts_with('(') {
        (vec![], rest)
    } else {
        // Require a closing paren — `qual:allow(iosp` (no close) is
        // malformed; falling back to `rest.len()` would treat the
        // tail as a valid dim list. Such markers route through
        // `detect_invalid_qual_allow` instead.
        let close_paren = rest.find(')')?;
        let dims_str = &rest[1..close_paren];
        let dimensions: Vec<Dimension> = dims_str
            .split(',')
            .filter_map(|s| Dimension::from_str_opt(s.trim()))
            .collect();
        let after_parens = rest.get(close_paren + 1..).map(str::trim).unwrap_or("");
        (dimensions, after_parens)
    };

    if dimensions.is_empty() {
        return None;
    }

    let reason = (!reason_text.is_empty())
        .then(|| extract_reason(reason_text))
        .flatten();

    Some(Suppression {
        line: line_number,
        dimensions,
        reason,
    })
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
