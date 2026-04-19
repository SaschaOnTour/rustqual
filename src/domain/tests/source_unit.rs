use crate::domain::SourceUnit;
use std::path::PathBuf;

#[test]
fn source_unit_holds_path_and_content() {
    let unit = SourceUnit::new(PathBuf::from("src/lib.rs"), "fn main() {}".into());
    assert_eq!(unit.path(), std::path::Path::new("src/lib.rs"));
    assert_eq!(unit.content(), "fn main() {}");
}

#[test]
fn equal_source_units_compare_equal() {
    let a = SourceUnit::new(PathBuf::from("x.rs"), "a".into());
    let b = SourceUnit::new(PathBuf::from("x.rs"), "a".into());
    assert_eq!(a, b);
}

#[test]
fn source_units_with_different_content_are_not_equal() {
    let a = SourceUnit::new(PathBuf::from("x.rs"), "a".into());
    let b = SourceUnit::new(PathBuf::from("x.rs"), "b".into());
    assert_ne!(a, b);
}

#[test]
fn source_units_with_different_paths_are_not_equal() {
    let a = SourceUnit::new(PathBuf::from("a.rs"), "x".into());
    let b = SourceUnit::new(PathBuf::from("b.rs"), "x".into());
    assert_ne!(a, b);
}
