//! Rule cards — the single source of truth for per-rule metadata.
//!
//! Every reporter that names a rule renders from this registry: the SARIF
//! `tool.driver.rules` table, `--explain <RULE-ID>`, and the text summary's
//! rule titles. One card per stable catalog id; adding a rule means adding a
//! card here (the SARIF sync test enforces set equality).

mod architecture;
mod boilerplate;
mod complexity;
mod coupling;
mod dry;
mod governance;
mod iosp;
mod srp;
mod structural;
mod tq;

/// Everything a consumer needs to act on a finding of one rule: what fired,
/// why it matters, how to fix it, how to suppress it as a last resort, and
/// which config knob governs it. All fields are mandatory prose — a rule
/// without a config knob or suppression says so explicitly instead of
/// leaving the reader to guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleCard {
    /// Stable catalog id (`BP-009`, `architecture/layer`, …).
    pub id: &'static str,
    /// One-line title, shared with the SARIF `shortDescription`.
    pub title: &'static str,
    /// What pattern the analyzer matches.
    pub detects: &'static str,
    /// Why the pattern is worth flagging.
    pub why: &'static str,
    /// Concrete fix forms, most preferred first.
    pub fix: &'static str,
    /// The copyable suppression marker (or why none exists).
    pub suppress: &'static str,
    /// The governing `rustqual.toml` knob (or why none exists).
    pub config: &'static str,
}

/// Card groups in catalog order (mirrors the SARIF rules table).
const GROUPS: &[&[RuleCard]] = &[
    iosp::CARDS,
    complexity::CARDS,
    dry::CARDS,
    boilerplate::CARDS,
    srp::CARDS,
    coupling::CARDS,
    structural::CARDS,
    tq::CARDS,
    architecture::CARDS,
    governance::CARDS,
];

/// Every rule card, in catalog order.
/// Operation: flattens the static groups.
pub fn all_rule_cards() -> impl Iterator<Item = &'static RuleCard> {
    GROUPS.iter().flat_map(|g| g.iter())
}

/// Look up one card by catalog id, case-insensitively (`bp-009` resolves
/// like `BP-009`). Integration: scans `all_rule_cards`.
pub fn find_rule_card(id: &str) -> Option<&'static RuleCard> {
    all_rule_cards().find(|c| c.id.eq_ignore_ascii_case(id))
}
