//! What a coverage report contributes: the seeded tested set, symbol
//! demangling, and which findings the report actually answered.

use std::collections::{HashMap, HashSet};

use super::make_declared;
use crate::adapters::analyzers::dry::dead_code::DeadCodeWarning;
use crate::adapters::analyzers::tq::untested::*;
use crate::adapters::analyzers::tq::TqWarningKind;

#[test]
fn coverage_seeds_the_tested_set() {
    // `FNDA:1,name` says a test ran the function. That is measurement, not
    // inference — it settles the cases the call graph cannot follow (a macro it
    // does not expand, a trait object, generated code) without any heuristic.
    let hits: HashMap<String, u64> = [("ran".to_string(), 3u64), ("never".to_string(), 0u64)]
        .into_iter()
        .collect();
    let data = crate::adapters::analyzers::tq::lcov::LcovFileData {
        function_hits: hits,
        line_hits: HashMap::new(),
    };
    let files: HashMap<String, _> = [("src/lib.rs".to_string(), data)].into_iter().collect();
    let seeded = crate::adapters::analyzers::tq::executed_under_test(Some(&files));
    assert!(seeded.contains(&"ran".to_string()));
    assert!(
        !seeded.contains(&"never".to_string()),
        "a recorded-but-never-executed function is not tested: {seeded:?}"
    );
    assert!(
        crate::adapters::analyzers::tq::executed_under_test(None).is_empty(),
        "no report, no change to the call-graph answer"
    );
}

#[test]
fn mangled_coverage_symbols_yield_their_function_name() {
    use crate::adapters::analyzers::tq::lcov::symbol_base_names;
    // `llvm-cov` writes v0-mangled symbols, so comparing them to a declared
    // function's bare name never matched — the report was read and then thrown
    // away. A closure or trait impl inside a function carries the outer name in
    // the middle, so every segment counts.
    let sym = "_RNvNtNtCs569pcWMmiue_17sv_test_contracts6suites19credential_provider20capture_secret_event";
    assert!(symbol_base_names(sym).contains(&"capture_secret_event".to_string()));
    // A symbol for a closure or trait impl *inside* the function runs the name
    // straight into the next segment (`…capture_secret_eventNtB2_9BufWriter…`),
    // so it yields a prefix rather than the name. Harmless: the function has its
    // own entry, which is the one above.
    let nested = "_RNvXNvNtNtCs569pcWMmiue_17sv_test_contracts6suites19credential_provider20capture_secret_eventNtB2_9BufWriterNtNtCsjk69aaednnH_3std2io5Write5flush";
    assert!(symbol_base_names(nested)
        .iter()
        .any(|n| n.starts_with("capture_secret_event")));
    assert_eq!(
        symbol_base_names("plain_name"),
        vec!["plain_name".to_string()]
    );
}

#[test]
fn a_monomorphised_symbol_yields_its_generic_function() {
    use crate::adapters::analyzers::tq::lcov::symbol_base_names;
    // The case a downstream issue asked about: LCOV records one symbol per
    // instantiation, and the name runs straight into the type arguments. Full
    // line coverage then cleared nothing, because no key ever matched.
    let sym = "_RINvNtNtCs569pcWMmiue_17sv_test_contracts6suites15run_event_store12append_ticksNtNtCs7gCM0XvToDQ_21sv_adp_storage_sqlite7adapter13SqliteStorageEB1j_";
    assert!(symbol_base_names(sym).contains(&"append_ticks".to_string()));
}

#[test]
fn a_digit_in_a_function_name_survives_demangling() {
    use crate::adapters::analyzers::tq::lcov::symbol_base_names;
    // Splitting the symbol on digits cut `sha256_digest` into `sha` and
    // `_digest`, so the function never appeared in the lookup — and with it
    // every name carrying a digit (`parse_v2`, `http2_connect`). The lengths
    // are read instead, which is what they are for.
    let sym = "_RNvCsgABWfZxqfrq_8rust_out13sha256_digest";
    let names = symbol_base_names(sym);
    assert!(names.contains(&"sha256_digest".to_string()), "{names:?}");
    assert!(names.contains(&"rust_out".to_string()), "{names:?}");
    assert!(
        !names.contains(&"sha".to_string()),
        "no split name: {names:?}"
    );
}

#[test]
fn a_finding_says_whether_the_report_covered_that_function() {
    // The whole point of asking per finding: one positive hit somewhere is no
    // evidence about a function the report never names. `target` is absent from
    // the report, `seen` is in it with no execution — the first was answered by
    // the call graph, the second by measurement, and a global "a report was
    // read" would have called both measured.
    let declared = [make_declared("target", false), make_declared("seen", false)];
    let prod_calls: HashSet<String> = ["target".to_string(), "seen".to_string()]
        .into_iter()
        .collect();
    let in_report: HashSet<String> = ["seen".to_string(), "unrelated_helper".to_string()]
        .into_iter()
        .collect();
    let out = detect_untested_functions(
        &declared,
        &prod_calls,
        &HashSet::new(),
        &[] as &[DeadCodeWarning],
        &in_report,
    );
    let evidence = |name: &str| {
        out.iter()
            .find(|w| w.function_name == name)
            .map(|w| w.kind.clone())
    };
    assert_eq!(
        evidence("target"),
        Some(TqWarningKind::Untested { measured: false }),
        "{out:?}"
    );
    assert_eq!(
        evidence("seen"),
        Some(TqWarningKind::Untested { measured: true }),
        "{out:?}"
    );
}
