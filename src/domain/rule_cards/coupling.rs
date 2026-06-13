//! Rule cards for the Coupling dimension (CP-*).

use super::RuleCard;

pub(super) const CARDS: &[RuleCard] = &[
    RuleCard {
        id: "CP-001",
        title: "Circular module dependency",
        detects: "A cycle in the module dependency graph (A uses B uses … \
            uses A).",
        why: "Modules in a cycle cannot be understood, tested, or replaced \
            independently — the cycle is effectively one large module.",
        fix: "Extract what both sides need into a third module, or invert \
            one edge with a trait defined on the consumer side (dependency \
            inversion).",
        suppress: "Not suppressible — circular dependencies always count \
            toward the score (there is deliberately no allow(coupling, \
            cycle) target).",
        config: "[coupling] enabled — the check has no threshold; a cycle \
            either exists or not.",
    },
    RuleCard {
        id: "CP-002",
        title: "Stable Dependencies Principle violation",
        detects: "A module depending on a module that is less stable \
            (higher instability I = Ce/(Ca+Ce)) than itself.",
        why: "When stable code depends on volatile code, every change in \
            the volatile module ripples into the code everyone else relies \
            on.",
        fix: "Invert the dependency with an interface owned by the stable \
            side, or move the volatile part out of the stable module's \
            dependency path.",
        suppress: "// qual:allow(coupling, sdp) reason: \"…\".",
        config: "[coupling] check_sdp (default true).",
    },
    RuleCard {
        id: "CP-003",
        title: "Module instability exceeds configured threshold",
        detects: "A module whose instability I = Ce/(Ca+Ce) exceeds \
            max_instability (default 0.8), or whose fan-in/fan-out exceed \
            max_fan_in/max_fan_out.",
        why: "A module that depends on many others while many depend on it \
            is a change amplifier: edits anywhere reach it, and its edits \
            reach everywhere.",
        fix: "Reduce outgoing dependencies (split the module, inject \
            dependencies via traits) or reduce incoming ones (move the \
            widely-used parts into a smaller, stable core).",
        suppress: "// qual:allow(coupling, max_instability=N) reason: \"…\" \
            (also max_fan_in=N / max_fan_out=N). Module-global metrics: the \
            marker is not line-anchored, so orphan detection skips it.",
        config: "[coupling] max_instability, max_fan_in, max_fan_out.",
    },
];
