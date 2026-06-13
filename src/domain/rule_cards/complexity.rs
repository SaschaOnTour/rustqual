//! Rule cards for the Complexity dimension (CX-*, A20).

use super::RuleCard;

pub(super) const CARDS: &[RuleCard] = &[
    RuleCard {
        id: "CX-001",
        title: "Cognitive complexity exceeds threshold",
        detects: "A function whose cognitive-complexity score (branching, \
            weighted by nesting depth) exceeds max_cognitive (default 15).",
        why: "Cognitive complexity measures how hard a body is to follow; \
            deeply nested decisions are where defects hide and reviews stall.",
        fix: "Extract decision branches into small named functions, flatten \
            nesting with early returns / let-else, or replace conditional \
            ladders with match dispatch or table lookups.",
        suppress: "// qual:allow(complexity, max_cognitive=N) reason: \"…\" — \
            pin at the current value; re-fires above N.",
        config: "[complexity] max_cognitive (default 15).",
    },
    RuleCard {
        id: "CX-002",
        title: "Cyclomatic complexity exceeds threshold",
        detects: "A function with more independent paths (branches, loops, \
            guards) than max_cyclomatic (default 10). Trivial match arms do \
            not increment the count.",
        why: "Every path needs a test; past the threshold the function can no \
            longer be covered or reasoned about exhaustively.",
        fix: "Split the function along its decision points; collapse repeated \
            branching into data-driven dispatch (match → lookup table) or \
            extract validation chains into helpers returning Result.",
        suppress: "// qual:allow(complexity, max_cyclomatic=N) reason: \"…\" — \
            pin at the current value; re-fires above N.",
        config: "[complexity] max_cyclomatic (default 10).",
    },
    RuleCard {
        id: "CX-003",
        title: "Magic number literal in non-const context",
        detects: "A numeric literal outside const/static context in \
            production code (test functions are exempt). Negative literals \
            count once; allowed_magic_numbers are skipped.",
        why: "An unnamed constant hides intent and invites divergent copies \
            of the same value across the codebase.",
        fix: "Name the value as a const next to its use (or in the module's \
            constants block) with a domain-meaningful name.",
        suppress: "// qual:allow(complexity, magic_numbers) reason: \"…\".",
        config: "[complexity] detect_magic_numbers, allowed_magic_numbers.",
    },
    RuleCard {
        id: "CX-004",
        title: "Function length exceeds threshold",
        detects: "A function body longer than max_function_lines (default \
            60). Applies to test functions too, at [tests].max_function_lines \
            (which defaults to the production value).",
        why: "Long functions bundle several jobs; they resist naming, reuse, \
            and review, and their tests blur what is actually asserted.",
        fix: "Extract cohesive steps into named helpers. In tests: pull setup \
            into builder/helper functions or split into focused tests.",
        suppress: "// qual:allow(complexity, max_function_lines=N) reason: \
            \"…\" — pin at the current value; re-fires above N.",
        config: "[complexity] max_function_lines; [tests] max_function_lines \
            overrides it for test code.",
    },
    RuleCard {
        id: "CX-005",
        title: "Nesting depth exceeds threshold",
        detects: "A function whose deepest block nesting exceeds \
            max_nesting_depth (default 4).",
        why: "Each nesting level multiplies the conditions a reader must hold \
            in their head to understand the innermost line.",
        fix: "Invert conditions into early returns, use let-else for \
            fallible bindings, extract the inner loop/match body into a \
            function.",
        suppress: "// qual:allow(complexity, max_nesting_depth=N) reason: \
            \"…\" — pin at the current value; re-fires above N.",
        config: "[complexity] max_nesting_depth (default 4).",
    },
    RuleCard {
        id: "CX-006",
        title: "Unsafe block detected",
        detects: "An unsafe block or unsafe fn in analyzed code.",
        why: "Each unsafe block is a manual proof obligation the compiler no \
            longer checks; the score keeps visible where those live.",
        fix: "Encapsulate the unsafe behind a small, audited safe API with a \
            // SAFETY: comment, or replace it with a safe alternative.",
        suppress: "// qual:allow(unsafe) reason: \"…\" — the dedicated narrow \
            marker for CX-006 (equivalently // qual:allow(complexity, unsafe)).",
        config: "[complexity] detect_unsafe (default true).",
    },
    RuleCard {
        id: "A20",
        title: "Error handling issue (unwrap/expect/panic/todo)",
        detects: "unwrap(), expect() (unless allow_expect = true), panic!, \
            todo!, or unimplemented! in production code (test functions are \
            exempt).",
        why: "Each is a crash path that bypasses Result-based error flow — an \
            input the author did not handle, not an error strategy.",
        fix: "Propagate with `?`, convert into a typed error (thiserror), or \
            handle the None/Err branch explicitly. For true invariants, \
            document via expect(\"invariant: …\") and set allow_expect = true.",
        suppress: "// qual:allow(complexity, error_handling) reason: \"…\".",
        config: "[complexity] detect_error_handling, allow_expect.",
    },
];
