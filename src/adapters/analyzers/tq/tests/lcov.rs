use crate::adapters::analyzers::tq::lcov::*;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn write_lcov(content: &str) -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("lcov.info");
    fs::write(&path, content).unwrap();
    (tmp, path)
}

#[test]
fn test_parse_basic_lcov() {
    let (_tmp, path) =
        write_lcov("SF:src/lib.rs\nFNDA:5,my_func\nDA:10,3\nDA:11,0\nend_of_record\n");
    let result = parse_lcov(&path).unwrap();
    assert!(result.contains_key("src/lib.rs"));
    let data = &result["src/lib.rs"];
    assert_eq!(data.function_hits.get("my_func"), Some(&5));
    assert_eq!(data.line_hits.get(&10), Some(&3));
    assert_eq!(data.line_hits.get(&11), Some(&0));
}

#[test]
fn test_parse_multiple_files() {
    let (_tmp, path) = write_lcov(
        "SF:src/a.rs\nFNDA:1,func_a\nend_of_record\nSF:src/b.rs\nFNDA:0,func_b\nend_of_record\n",
    );
    let result = parse_lcov(&path).unwrap();
    assert_eq!(result.len(), 2);
    assert!(result.contains_key("src/a.rs"));
    assert!(result.contains_key("src/b.rs"));
}

#[test]
fn test_parse_empty_file() {
    let (_tmp, path) = write_lcov("");
    let result = parse_lcov(&path).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_parse_malformed_lines_skipped() {
    let (_tmp, path) = write_lcov(
        "SF:src/lib.rs\nFNDA:not_a_number,func\nDA:bad\nFNDA:3,good_func\nend_of_record\n",
    );
    let result = parse_lcov(&path).unwrap();
    let data = &result["src/lib.rs"];
    assert!(!data.function_hits.contains_key("func"));
    assert_eq!(data.function_hits.get("good_func"), Some(&3));
}

#[test]
fn test_parse_missing_file_error() {
    let result = parse_lcov(Path::new("/nonexistent/lcov.info"));
    assert!(result.is_err());
}

#[test]
fn test_parse_da_with_checksum() {
    let (_tmp, path) = write_lcov("SF:src/lib.rs\nDA:15,2,abc123\nend_of_record\n");
    let result = parse_lcov(&path).unwrap();
    assert_eq!(result["src/lib.rs"].line_hits.get(&15), Some(&2));
}

#[test]
fn test_parse_no_end_of_record() {
    let (_tmp, path) = write_lcov("SF:src/lib.rs\nFNDA:1,func\nDA:5,1\n");
    let result = parse_lcov(&path).unwrap();
    assert!(result.contains_key("src/lib.rs"));
    assert_eq!(result["src/lib.rs"].function_hits.get("func"), Some(&1));
}

#[test]
fn test_parse_zero_hit_function() {
    let (_tmp, path) = write_lcov("SF:src/lib.rs\nFNDA:0,uncovered_fn\nend_of_record\n");
    let result = parse_lcov(&path).unwrap();
    assert_eq!(
        result["src/lib.rs"].function_hits.get("uncovered_fn"),
        Some(&0)
    );
}

#[test]
fn test_parse_unknown_line_within_record_does_not_split() {
    // `LF:1` matches none of SF/FNDA/DA and is not `end_of_record`, so it must
    // be ignored *while a file is open* — a record is closed only on a real
    // `end_of_record`, never merely because a file is currently open. Both line
    // hits must land in the single `src/lib.rs` record.
    let (_tmp, path) = write_lcov("SF:src/lib.rs\nDA:1,1\nLF:1\nDA:2,1\nend_of_record\n");
    let result = parse_lcov(&path).unwrap();
    assert_eq!(
        result.len(),
        1,
        "an unknown mid-record line must not open a new record"
    );
    let data = &result["src/lib.rs"];
    assert_eq!(data.line_hits.get(&1), Some(&1));
    assert_eq!(data.line_hits.get(&2), Some(&1));
}

#[test]
fn a_file_the_run_measured_nothing_in_is_not_measured_coverage() {
    // Readable and parseable is not the same as usable. Any text file parses
    // into an empty result — an LLVM-IR dump handed to `--coverage` produced
    // `"coverage": "measured"` while the analysis had fallen back to the call
    // graph entirely, which is exactly the belief the flag exists to prevent.
    let (_tmp, path) = write_lcov("define i32 @main() {\n  ret i32 0\n}\n");
    assert!(!crate::adapters::analyzers::tq::coverage_is_measured(&path));
}

#[test]
fn a_report_that_recorded_no_execution_is_still_measurement() {
    // `FNDA:0,my_func` is an answer, not a silence: the run recorded that the
    // function never ran. Asking for a *positive* hit made the run-level flag
    // say "call-graph-only" while the finding for that very function said
    // "measured". Both questions are now decided by the same set — the names
    // the report mentions — so they cannot contradict each other.
    let (_tmp, path) = write_lcov("SF:src/lib.rs\nFNDA:0,my_func\nend_of_record\n");
    assert!(crate::adapters::analyzers::tq::coverage_is_measured(&path));
}

#[test]
fn a_report_with_an_executed_function_is_measured_coverage() {
    let (_tmp, path) = write_lcov("SF:src/lib.rs\nFNDA:5,my_func\nend_of_record\n");
    assert!(crate::adapters::analyzers::tq::coverage_is_measured(&path));
}

#[test]
fn a_missing_report_is_not_measured_coverage() {
    assert!(!crate::adapters::analyzers::tq::coverage_is_measured(
        Path::new("/nonexistent/lcov.info")
    ));
}
