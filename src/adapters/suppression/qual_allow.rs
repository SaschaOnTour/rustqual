// Re-export Domain types so existing `crate::findings::{Dimension, Suppression}`
// call sites keep working. The canonical definitions live in `crate::domain`.
use crate::domain::{target_kind, target_names, SuppressionTarget, TargetKind};
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
/// duplicate checks, which stay active. It does not count against
/// `max_suppression_ratio`.
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
        Some(Suppression::blanket(
            line_number,
            vec![Dimension::Iosp],
            reason,
        ))
    } else {
        None
    }
}

// qual:api
/// Why a `// qual:allow(...)` marker was flagged as invalid. Carried
/// through the side-channel into the orphan-finding's reason text so
/// the author sees the actual failure mode (unknown dim vs unclosed
/// parens), not a generic "did not parse" message that lies for the
/// unclosed-with-valid-dim case (`// qual:allow(iosp`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidQualAllow {
    /// Parens closed, but the first comma-separated entry does not resolve
    /// to a known dimension (removed dim, stray text).
    UnknownDimensions(String),
    /// Opening `(` present but no closing `)`. Surfaced regardless
    /// of whether the tail spells a valid dim — the marker's shape
    /// is broken and the parser rejects it, so it must surface.
    UnclosedParens(String),
    /// A `allow(dim, name)` whose `name` is not a known target of `dim`.
    /// Carries the valid target list so the author sees what to write.
    UnknownTarget {
        dim: String,
        target: String,
        valid: Vec<String>,
    },
    /// A threshold-metric target used without its mandatory `=value`.
    MetricNeedsValue { dim: String, target: String },
    /// A boolean target given a `=value` it does not accept.
    BooleanTakesNoValue { dim: String, target: String },
    /// A metric pin value that did not parse as a number.
    BadPinValue { target: String, value: String },
    /// A targeted suppression without the mandatory `reason:`.
    TargetNeedsReason { dim: String, target: String },
    /// A bare `allow(dim)` (no target) for a multi-kind dimension. The
    /// targeted form is required so the suppression names what it silences.
    BlanketNotAllowed { dim: String, valid: Vec<String> },
}

impl InvalidQualAllow {
    /// Renderable reason for the orphan-finding's `reason` field.
    /// Operation: per-variant message formatting (dispatch, no own calls).
    pub fn reason(&self) -> String {
        match self {
            Self::UnknownDimensions(spec) => {
                format!("invalid qual:allow — '{spec}' did not parse to any known dimension")
            }
            Self::UnclosedParens(spec) => format!(
                "invalid qual:allow — marker has unclosed parens (missing `)`); content was '{spec}'"
            ),
            Self::UnknownTarget { dim, target, valid } => format!(
                "invalid qual:allow — '{target}' is not a known target of {dim}; valid targets: {}",
                valid.join(", ")
            ),
            Self::MetricNeedsValue { dim, target } => format!(
                "invalid qual:allow — {dim} target '{target}' is a threshold metric and needs a value, e.g. {target}=<n>"
            ),
            Self::BooleanTakesNoValue { dim, target } => format!(
                "invalid qual:allow — {dim} target '{target}' is a boolean finding and takes no value"
            ),
            Self::BadPinValue { target, value } => {
                format!("invalid qual:allow — pin value '{value}' for '{target}' is not a number")
            }
            Self::TargetNeedsReason { dim, target } => format!(
                "invalid qual:allow — targeted suppression ({dim}, {target}) requires a reason: \"…\""
            ),
            Self::BlanketNotAllowed { dim, valid } => format!(
                "invalid qual:allow — bare ({dim}) would silence every {dim} finding; name what you mean: allow({dim}, <target>). Targets: {}",
                valid.join(", ")
            ),
        }
    }
}

/// Detect a `// qual:allow(...)` marker whose parens are malformed
/// (missing close-paren, e.g. `// qual:allow(iosp`) OR contain text
/// but no recognized dimension name (typo: `srp_params`, removed
/// dim, stray text). Returns the typed failure kind for orphan-
/// finding text. Bare `// qual:allow`, `// qual:allow()`, and the
/// special-purpose `// qual:allow(unsafe)` form are NOT flagged —
/// the first two carry no intent, the third is its own annotation
/// handled by `is_unsafe_allow_marker`. Must agree with
/// `parse_qual_allow`'s reject path so every malformed marker
/// either suppresses or surfaces as orphan, never both, never
/// silently dropped.
pub fn detect_invalid_qual_allow(trimmed: &str) -> Option<InvalidQualAllow> {
    if is_unsafe_allow_marker(trimmed) {
        return None;
    }
    let rest = trimmed.strip_prefix("// qual:allow")?;
    match classify_qual_allow(0, rest) {
        AllowParse::Invalid(invalid) => Some(invalid),
        _ => None,
    }
}

/// Parse the part after "// qual:allow" into a usable suppression, or `None`
/// for the no-intent forms (bare `allow`, `allow()`) and every invalid form
/// — the latter surface separately via `detect_invalid_qual_allow`. Both
/// route through the single `classify_qual_allow` so they cannot disagree.
/// Operation: keeps the `Valid` outcome (dispatch, own call reclassified).
fn parse_qual_allow(line_number: usize, rest: &str) -> Option<Suppression> {
    match classify_qual_allow(line_number, rest) {
        AllowParse::Valid(suppression) => Some(suppression),
        _ => None,
    }
}

/// Outcome of classifying the text after `// qual:allow`.
enum AllowParse {
    /// A usable suppression — blanket `allow(dim)` or targeted `allow(dim, t)`.
    Valid(Suppression),
    /// No intent: bare `allow` / `allow()`. Neither suppresses nor surfaces.
    Bare,
    /// Malformed or unknown — surfaced as `ORPHAN_SUPPRESSION`.
    Invalid(InvalidQualAllow),
}

/// Single source of truth for `parse_qual_allow` (keeps `Valid`) and
/// `detect_invalid_qual_allow` (keeps `Invalid`), so they can never diverge.
/// Splits off the parens, extracts the reason, and delegates the comma
/// entries.
/// Operation: paren/shape dispatch; entry classification + reason extraction
/// reach non-violation helpers.
fn classify_qual_allow(line: usize, rest: &str) -> AllowParse {
    let rest = rest.trim();
    if rest.is_empty() || !rest.starts_with('(') {
        return AllowParse::Bare;
    }
    let Some(close) = rest.find(')') else {
        return AllowParse::Invalid(InvalidQualAllow::UnclosedParens(
            rest[1..].trim().to_string(),
        ));
    };
    let inner = rest[1..close].trim();
    if inner.is_empty() {
        return AllowParse::Bare;
    }
    let after = rest.get(close + 1..).map(str::trim).unwrap_or("");
    let reason = (!after.is_empty()).then(|| extract_reason(after)).flatten();
    classify_entries(line, inner, reason)
}

/// Classify the comma-separated entries. The first must be a dimension; the
/// rest are either more dimensions (bare multi-dim) or a single target.
/// Operation: entry dispatch reaching non-violation helpers.
fn classify_entries(line: usize, inner: &str, reason: Option<String>) -> AllowParse {
    let entries: Vec<&str> = inner.split(',').map(str::trim).collect();
    let Some(dim0) = Dimension::from_str_opt(entries[0]) else {
        return AllowParse::Invalid(InvalidQualAllow::UnknownDimensions(inner.to_string()));
    };
    let extra = &entries[1..];
    if extra.is_empty() {
        return blanket_or_invalid(line, vec![dim0], reason);
    }
    // An entry is "target-like" if it pins a value or is not a dimension.
    let target_like = |e: &&str| e.contains('=') || Dimension::from_str_opt(e).is_none();
    if !extra.iter().any(target_like) {
        let dims = std::iter::once(dim0)
            .chain(extra.iter().filter_map(|e| Dimension::from_str_opt(e)))
            .collect();
        return blanket_or_invalid(line, dims, reason);
    }
    classify_target(line, dim0, extra, reason)
}

/// A bare (untargeted) `allow(dim)` is valid only for dimensions with no
/// targets (`iosp`). For a multi-kind dimension it must name a target, so the
/// suppression says what it silences — otherwise it is rejected.
/// Operation: target-vocabulary check, no own calls.
fn blanket_or_invalid(line: usize, dims: Vec<Dimension>, reason: Option<String>) -> AllowParse {
    if let Some(dim) = dims.iter().copied().find(|d| !target_names(*d).is_empty()) {
        return AllowParse::Invalid(InvalidQualAllow::BlanketNotAllowed {
            dim: dim.to_string(),
            valid: target_names(dim).iter().map(|s| s.to_string()).collect(),
        });
    }
    AllowParse::Valid(Suppression::blanket(line, dims, reason))
}

/// Classify a single `name` or `name=value` target against the dimension's
/// vocabulary. Enforces metric⇒value, boolean⇒no-value, and a mandatory
/// reason. More than one extra entry alongside a target is an unknown-target
/// error (dimensions and a target cannot be mixed). The dimension's canonical
/// name (via `Display`) is used in error text, so no raw string is threaded.
/// Operation: vocabulary dispatch reaching non-violation helpers.
fn classify_target(
    line: usize,
    dim: Dimension,
    extra: &[&str],
    reason: Option<String>,
) -> AllowParse {
    let unknown = |target: String| {
        AllowParse::Invalid(InvalidQualAllow::UnknownTarget {
            dim: dim.to_string(),
            target,
            valid: target_names(dim).iter().map(|s| s.to_string()).collect(),
        })
    };
    if extra.len() != 1 {
        return unknown(extra.join(", "));
    }
    let (name, value) = match extra[0].split_once('=') {
        Some((n, v)) => (n.trim(), Some(v.trim())),
        None => (extra[0], None),
    };
    match target_kind(dim, name) {
        None => unknown(name.to_string()),
        Some(TargetKind::Metric) => classify_metric(line, dim, name, value, reason),
        Some(TargetKind::Boolean) => classify_boolean(line, dim, name, value, reason),
    }
}

/// Build a metric (pinned) target: value mandatory + parseable, reason
/// mandatory.
/// Operation: value/reason validation, no own calls.
fn classify_metric(
    line: usize,
    dim: Dimension,
    name: &str,
    value: Option<&str>,
    reason: Option<String>,
) -> AllowParse {
    let Some(value) = value else {
        return AllowParse::Invalid(InvalidQualAllow::MetricNeedsValue {
            dim: dim.to_string(),
            target: name.to_string(),
        });
    };
    let Ok(pin) = value.parse::<f64>() else {
        return AllowParse::Invalid(InvalidQualAllow::BadPinValue {
            target: name.to_string(),
            value: value.to_string(),
        });
    };
    if reason.is_none() {
        return AllowParse::Invalid(InvalidQualAllow::TargetNeedsReason {
            dim: dim.to_string(),
            target: name.to_string(),
        });
    }
    AllowParse::Valid(Suppression {
        line,
        dimensions: vec![dim],
        reason,
        target: Some(SuppressionTarget::Metric {
            name: name.to_string(),
            pin,
        }),
    })
}

/// Build a boolean target: value rejected, reason mandatory.
/// Operation: value/reason validation, no own calls.
fn classify_boolean(
    line: usize,
    dim: Dimension,
    name: &str,
    value: Option<&str>,
    reason: Option<String>,
) -> AllowParse {
    if value.is_some() {
        return AllowParse::Invalid(InvalidQualAllow::BooleanTakesNoValue {
            dim: dim.to_string(),
            target: name.to_string(),
        });
    }
    if reason.is_none() {
        return AllowParse::Invalid(InvalidQualAllow::TargetNeedsReason {
            dim: dim.to_string(),
            target: name.to_string(),
        });
    }
    AllowParse::Valid(Suppression {
        line,
        dimensions: vec![dim],
        reason,
        target: Some(SuppressionTarget::Boolean {
            name: name.to_string(),
        }),
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
