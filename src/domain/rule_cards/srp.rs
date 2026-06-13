//! Rule cards for the SRP dimension (SRP-*).

use super::RuleCard;

pub(super) const CARDS: &[RuleCard] = &[
    RuleCard {
        id: "SRP-001",
        title: "Struct may violate Single Responsibility Principle",
        detects: "A struct whose composite smell score crosses \
            smell_threshold — inputs: field count (max_fields), method count \
            (max_methods), fan-out (max_fan_out), and LCOM4 cohesion \
            (lcom4_threshold). Also fires on test-file structs (a \
            god-fixture is a real smell), at production thresholds.",
        why: "A struct accumulating fields, methods, and dependencies has \
            several reasons to change; every change risks the unrelated \
            responsibilities sharing its state.",
        fix: "Split along the LCOM4 method clusters: each disconnected \
            cluster of methods+fields is a candidate type. Extract \
            collaborating field groups into sub-structs the original \
            composes.",
        suppress: "// qual:allow(srp, god_struct) reason: \"…\".",
        config: "[srp] smell_threshold, max_fields, max_methods, \
            max_fan_out, lcom4_threshold, weights.",
    },
    RuleCard {
        id: "SRP-002",
        title: "Module has excessive production code length",
        detects: "A file longer than file_length (default 300 lines). Test \
            files are checked too, at [tests].file_length (defaults to the \
            production value). Production files additionally fire when they \
            contain more than max_independent_clusters unrelated \
            free-function groups (module cohesion; production-only).",
        why: "A long file is where unrelated concerns quietly co-locate; \
            independent clusters in one module are separate modules wearing \
            one name.",
        fix: "Split along behavior seams into focused modules; the \
            independent clusters named by the finding are the natural cut \
            lines.",
        suppress: "// qual:allow(srp, file_length=N) reason: \"…\" — or the \
            max_independent_clusters=N target for the cluster variant; pins \
            re-fire above N.",
        config: "[srp] file_length, max_independent_clusters, \
            min_cluster_statements; [tests] file_length for test files.",
    },
    RuleCard {
        id: "SRP-003",
        title: "Function has too many parameters — reduce parameter count",
        detects: "A function signature with more than max_parameters \
            parameters (default 5).",
        why: "Long parameter lists are a missing type: callers pass loose \
            values whose relationships and invariants nothing checks.",
        fix: "Group related parameters into a struct (often revealing a \
            domain concept), or split the function if the parameters serve \
            different jobs.",
        suppress: "// qual:allow(srp, max_parameters=N) reason: \"…\" — pin \
            re-fires above N.",
        config: "[srp] max_parameters (default 5).",
    },
];
