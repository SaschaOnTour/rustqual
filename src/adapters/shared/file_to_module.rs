/// Convert a file path to its top-level module name.
/// Operation: string manipulation logic, no own calls.
///
/// Examples:
/// - `main.rs` → `main`
/// - `config/mod.rs` → `config`
/// - `analyzer/types.rs` → `analyzer`
/// - `src/pipeline.rs` → `pipeline`
pub fn file_to_module(file_path: &str) -> String {
    let path = file_path.replace('\\', "/");
    let stripped = path.strip_prefix("src/").unwrap_or(&path);
    if let Some(slash_pos) = stripped.find('/') {
        stripped[..slash_pos].to_string()
    } else {
        stripped.strip_suffix(".rs").unwrap_or(stripped).to_string()
    }
}
