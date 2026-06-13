//! Rule cards for suppression governance (SUP-*, ORPHAN-*).

use super::RuleCard;

pub(super) const CARDS: &[RuleCard] = &[
    RuleCard {
        id: "SUP-001",
        title: "Suppression ratio exceeds configured maximum",
        detects: "The ratio of suppression markers (qual:allow + #[allow]) \
            to functions exceeds max_suppression_ratio (default 0.05).",
        why: "Each marker is debt; past a ratio the tool is being silenced \
            rather than satisfied, and the score stops meaning anything.",
        fix: "Fix findings instead of allowing them; delete stale markers \
            (ORPHAN-001 lists them). Raising max_suppression_ratio is a \
            deliberate, visible decision — make it in review, not ad hoc.",
        suppress: "Not suppressible — this is the meta-check on \
            suppressions themselves. It warns by default and fails only \
            with --fail-on-warnings.",
        config: "max_suppression_ratio (top level, default 0.05).",
    },
    RuleCard {
        id: "ORPHAN-001",
        title: "Stale qual:allow marker: no finding in the annotation window",
        detects: "A qual:allow marker that matches no finding of its kind \
            in its comment window (stale), or a metric pin sitting more \
            than [suppression].pin_headroom above the value it covers \
            (too-loose).",
        why: "A stale marker silently pre-authorizes a future regression; \
            a too-loose pin absorbs growth up to its ceiling without \
            anyone deciding that.",
        fix: "Delete the stale marker; tighten a too-loose pin to the \
            current metric value. If the marker was a typo (wrong target \
            name), write the intended one — rustqual --explain allow lists \
            the vocabulary.",
        suppress: "Not suppressible — the marker itself is the problem; \
            the only fix is editing or removing it.",
        config: "[suppression] pin_headroom (default 0.10) governs the \
            too-loose threshold.",
    },
];
