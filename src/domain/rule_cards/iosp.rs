//! Rule card for the IOSP dimension (single-kind).

use super::RuleCard;

pub(super) const CARDS: &[RuleCard] = &[RuleCard {
    id: "iosp/violation",
    title: "Function violates the Integration Operation Segregation Principle",
    detects: "A function that both calls other project functions AND contains \
        its own logic (if/for/match/computation) — neither a pure Integration \
        (orchestration only) nor a pure Operation (logic only).",
    why: "Mixed functions interleave decisions with delegation, so neither the \
        flow nor the logic can be read, tested, or reused on its own; defects \
        hide in the seams between the two.",
    fix: "Extract the logic into Operation functions and keep this function as \
        a pure orchestrator — or inline the orchestration so the function \
        becomes a pure Operation. In lenient mode (default), closures and \
        iterator chains do not count as logic; for-loops and matches whose \
        every arm only delegates count as dispatch.",
    suppress: "// qual:allow(iosp) reason: \"…\" — the bare form (iosp has no \
        sub-targets); legacy alias // iosp:allow.",
    config: "[weights] iosp sets the dimension weight; --strict-closures / \
        --strict-iterators tighten the leniency rules.",
}];
