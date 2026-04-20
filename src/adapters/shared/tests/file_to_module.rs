use crate::adapters::shared::file_to_module::file_to_module;

#[test]
fn test_file_to_module_root_file() {
    assert_eq!(file_to_module("main.rs"), "main");
    assert_eq!(file_to_module("pipeline.rs"), "pipeline");
}

#[test]
fn test_file_to_module_subdir_mod() {
    assert_eq!(file_to_module("config/mod.rs"), "config");
    assert_eq!(file_to_module("analyzer/mod.rs"), "analyzer");
}

#[test]
fn test_file_to_module_subdir_file() {
    assert_eq!(file_to_module("analyzer/types.rs"), "analyzer");
    assert_eq!(file_to_module("report/text.rs"), "report");
}

#[test]
fn test_file_to_module_src_prefix() {
    assert_eq!(file_to_module("src/main.rs"), "main");
    assert_eq!(file_to_module("src/config/mod.rs"), "config");
    assert_eq!(file_to_module("src/analyzer/types.rs"), "analyzer");
}

#[test]
fn test_file_to_module_backslash() {
    assert_eq!(file_to_module("src\\config\\mod.rs"), "config");
    assert_eq!(file_to_module("analyzer\\types.rs"), "analyzer");
}
