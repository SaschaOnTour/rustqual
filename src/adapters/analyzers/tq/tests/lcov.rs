use crate::adapters::analyzers::tq::lcov::*;
use std::collections::HashMap;
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
