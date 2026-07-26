//! Rule cards for the DRY dimension (DRY-*).

use super::RuleCard;

pub(super) const CARDS: &[RuleCard] = &[
    RuleCard {
        id: "DRY-001",
        title: "Duplicate function detected",
        detects: "Two or more functions whose normalized bodies are at least \
            similarity_threshold alike (default 0.85; exact duplicates score \
            1.0). Runs on test code too.",
        why: "Parallel copies drift: a fix lands in one and not the other, \
            and reviewers must diff by eye to see whether they still match.",
        fix: "Extract one shared helper and call it from every site. For \
            deliberate mirror pairs (encode/decode, serialize/deserialize), \
            mark one side with // qual:inverse(other_fn) instead.",
        suppress: "// qual:allow(dry, duplicate) reason: \"…\".",
        config: "[duplicates] similarity_threshold, min_tokens, \
            ignore_trait_impls.",
    },
    RuleCard {
        id: "DRY-002",
        title: "Dead code detected",
        detects: "A production function never called from production code: \
            'uncalled' (no callers at all) or 'testonly' (only tests reach \
            it). Macro bodies are scanned, so calls inside vec!/format!/rsx! \
            count as references.",
        why: "Dead code is maintained, reviewed, and refactored without ever \
            running — pure carrying cost that also misleads readers about \
            what the system does.",
        fix: "Delete the function, or wire it to its intended caller. Public \
            API entry points: mark // qual:api. Integration-test helpers \
            living in src/: mark // qual:test_helper.",
        suppress: "// qual:api (public API) or // qual:test_helper — DRY-002 \
            is deliberately NOT suppressible via qual:allow(dry, …).",
        config: "[duplicates] detect_dead_code (default true).",
    },
    RuleCard {
        id: "DRY-006",
        title: "Dead type or constant detected",
        detects: "A struct, enum, union, type alias, const or static that \
            nothing refers to: 'unused type' (no reference anywhere) or \
            'type test-only' (only test code names it). References are counted \
            by name across the whole workspace, including inside macro bodies \
            and attribute arguments. A type's own impl blocks do not count — \
            carrying methods keeps nothing alive.",
        why: "An unused type is read, maintained and refactored like the rest \
            of the code while modelling nothing. rustc's own dead_code lint \
            stops at the crate boundary, so a pub type nobody in the workspace \
            uses stays invisible there.",
        fix: "Delete it, or wire it to its intended user. Public API types: \
            mark // qual:api. Types serving integration tests from src/: mark \
            // qual:test_helper. Deliberate exceptions: #[allow(dead_code)].",
        suppress: "// qual:api (public API) or // qual:test_helper — DRY-006 \
            is deliberately NOT suppressible via qual:allow(dry, …), matching \
            DRY-002.",
        config: "[duplicates] detect_dead_types (default true).",
    },
    RuleCard {
        id: "DRY-003",
        title: "Duplicate code fragment",
        detects: "The same statement sequence (at least min_lines lines, \
            default 5) repeated across function bodies, in production and \
            test code.",
        why: "Fragment-level copies are the early stage of DRY-001: each \
            repetition is a future divergence point.",
        fix: "Extract the fragment into a named function; if the copies vary \
            only in values, parameterize them.",
        suppress: "// qual:allow(dry, fragment) reason: \"…\".",
        config: "[duplicates] min_lines, min_statements.",
    },
    RuleCard {
        id: "DRY-004",
        title: "Wildcard import (use module::*)",
        detects: "A glob import in production code. Prelude globs (any path \
            with a ::prelude segment) are skipped by this check's own logic; \
            test files are exempt.",
        why: "Globs hide where names come from, create silent collisions, \
            and let an upstream addition change this module's meaning.",
        fix: "Import the used names explicitly; group with braces if the \
            list is long.",
        suppress: "// qual:allow(dry, wildcard_imports) reason: \"…\".",
        config: "[duplicates] detect_wildcard_imports (default true).",
    },
    RuleCard {
        id: "DRY-005",
        title: "Repeated match pattern across functions",
        detects: "The same enum matched arm-by-arm in several functions \
            (3+ arms in 3+ places), in production and test code.",
        why: "Scattered matches over one enum mean every new variant demands \
            a synchronized edit in N places — a missed one is a logic bug.",
        fix: "Centralize the dispatch: one function owning the match, or move \
            the behavior into the enum as a method (one impl arm per \
            variant).",
        suppress: "// qual:allow(dry, repeated_matches) reason: \"…\".",
        config: "[duplicates] detect_repeated_matches (default true).",
    },
];
