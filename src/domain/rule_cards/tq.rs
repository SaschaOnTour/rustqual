//! Rule cards for the Test-Quality dimension (TQ-*).

use super::RuleCard;

pub(super) const CARDS: &[RuleCard] = &[
    RuleCard {
        id: "TQ-001",
        title: "Test function has no assertions",
        detects: "A test function without any assert-family macro, \
            should_panic expectation, or configured extra assertion macro. \
            Tests inside proptest!/quickcheck! bodies are recognized too.",
        why: "A test that asserts nothing can only fail by panicking — it \
            documents that the code ran, not that it was right.",
        fix: "Assert on observable behavior (return value, state change, \
            emitted output). If the point is 'does not panic', still assert \
            the result so regressions surface as failures, not silence.",
        suppress: "// qual:allow(test_quality, no_assertion) reason: \"…\".",
        config: "[test_quality] extra_assertion_macros extends what counts \
            as an assertion (e.g. verify!, check!).",
    },
    RuleCard {
        id: "TQ-002",
        title: "Test function does not call any production function",
        detects: "A test that never calls into production code — macro \
            bodies are scanned, so a SUT call inside assert!(…) or rsx!{…} \
            counts.",
        why: "A test without a system-under-test only tests its own \
            fixtures; it passes forever and verifies nothing.",
        fix: "Call the unit under test directly and assert on its result; \
            if the test exists to exercise a helper, test the production \
            caller instead.",
        suppress: "// qual:allow(test_quality, no_sut) reason: \"…\".",
        config: "[test_quality] enabled — the check itself has no \
            threshold.",
    },
    RuleCard {
        id: "TQ-003",
        title: "Production function is untested",
        detects: "A production function no test reaches through the call \
            graph (visitor-dispatch edges and macro-embedded calls are \
            resolved).",
        why: "Unreached code has no safety net: refactorings and \
            regressions in it are invisible to the suite.",
        fix: "Add a test calling it directly, or through a covered caller. \
            Public API entry points: mark // qual:api. Integration-test \
            helpers in src/: // qual:test_helper. Functions referenced only \
            dynamically (e.g. serde default = \"fn\") need a direct test \
            call — the call graph cannot see string references.",
        suppress: "// qual:allow(test_quality, untested) reason: \"…\".",
        config: "[test_quality] enabled; // qual:api and // qual:test_helper \
            exclude entry points without ratio cost.",
    },
    RuleCard {
        id: "TQ-004",
        title: "Production function has no coverage",
        detects: "A production function with zero covered lines in the LCOV \
            file passed via --coverage (functions absent from the LCOV data \
            are skipped, not flagged).",
        why: "Call-graph reachability says a test could reach the function; \
            coverage says none actually did — the stronger signal.",
        fix: "Exercise the function in a test that actually runs it; if it \
            is only reachable behind a cfg/feature, run the suite with that \
            configuration.",
        suppress: "// qual:allow(test_quality, uncovered) reason: \"…\".",
        config: "--coverage <lcov file> enables the check; [test_quality] \
            coverage_file sets a default path.",
    },
    RuleCard {
        id: "TQ-005",
        title: "Untested logic branches (uncovered lines)",
        detects: "A covered production function that still has uncovered \
            lines — branches the suite never takes (requires --coverage).",
        why: "The uncovered branches are usually the error paths and edge \
            cases — exactly where defects live.",
        fix: "Add cases for the uncovered branches: error returns, else \
            arms, early exits. The finding lists the uncovered lines.",
        suppress: "// qual:allow(test_quality, untested_logic) reason: \"…\".",
        config: "--coverage <lcov file> enables the check; [test_quality] \
            coverage_file sets a default path.",
    },
];
