//! Rule cards for the DRY dimension (DRY-*).

use super::RuleCard;

pub(super) const CARDS: &[RuleCard] = &[
    RuleCard {
        id: "DRY-001",
        title: "Duplicate function detected",
        detects: "Two or more functions whose normalized bodies are at least \
            similarity_threshold alike (default 0.85; exact duplicates score \
            1.0). Runs on test code too. Normalisation erases literal values \
            and renames locals and parameters positionally, but keeps the \
            names in call position — a called function, a method, a field — \
            so two bodies that call different functions are different bodies. \
            A qualified callee (mod::f(x)) is the one exception: it \
            contributes no token at all.",
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
            count as references. A macro that applies a metavariable \
            ($test(&$make())), or forwards one to a macro that does, counts \
            every ident at its invocation as a possible callee — the \
            functions a run_suite!(make; a, b) really runs arrive as bare \
            names nothing syntactic marks as calls.",
        why: "Dead code is maintained, reviewed, and refactored without ever \
            running — pure carrying cost that also misleads readers about \
            what the system does.",
        fix: "Delete the function, or wire it to its intended caller. Public \
            API entry points: mark // qual:api. Integration-test helpers \
            living in src/: mark // qual:test_helper. Exports \
            (#[no_mangle], #[used], #[export_name]) are already exempt — \
            their caller is the linker.",
        suppress: "// qual:api (public API) or // qual:test_helper — DRY-002 \
            is deliberately NOT suppressible via qual:allow(dry, …).",
        config: "[duplicates] detect_dead_code (default true).",
    },
    RuleCard {
        id: "DRY-006",
        title: "Dead type or constant detected",
        detects: "A struct, enum, union, type alias, const or static that \
            nothing reaches: 'unused type' (no reference anywhere) or 'type test-only' \
            (only test code reaches it). References are counted by name across \
            the whole workspace, including inside macro bodies and attribute \
            arguments, and inside doc-test fences (that code is compiled by \
            cargo test). Being mentioned is not enough: a reference counts \
            only if whatever made it is reached in turn, so types that name \
            each other in a cycle of any length keep nothing alive. A type's \
            own impl blocks do not count either — carrying methods keeps \
            nothing alive, and what those methods name lives or dies with the \
            type.",
        why: "An unused type is read, maintained and refactored like the rest \
            of the code while modelling nothing. rustc's own dead_code lint \
            stops at the crate boundary, so a pub type nobody in the workspace \
            uses stays invisible there.",
        fix: "Delete it, or wire it to its intended user. Public API types: \
            mark // qual:api — one marker on the entry point clears everything it \
            reaches. Types serving integration tests from src/: mark \
            // qual:test_helper. Deliberate exceptions: #[allow(dead_code)]. \
            Exports (#[no_mangle], #[used], #[export_name]) are already \
            exempt — their caller is the linker.",
        suppress: "// qual:api (public API) or // qual:test_helper — DRY-006 \
            is deliberately NOT suppressible via qual:allow(dry, …), matching \
            DRY-002.",
        config: "[duplicates] detect_dead_types (default true). Two blind \
            spots, both erring toward a finding: a file rustqual cannot parse \
            is dropped from the analysis (check stderr for parse warnings \
            before acting on a surprising batch), and content pulled in by \
            include! is never parsed at all. An unclosed doc fence errs the \
            other way — it leaks into the next item, suppressing findings.",
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
