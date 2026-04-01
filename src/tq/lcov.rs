use std::collections::HashMap;
use std::path::Path;

/// Per-file coverage data extracted from an LCOV file.
#[derive(Debug, Clone, Default)]
pub struct LcovFileData {
    /// Function hit counts: function_name → execution count (from FNDA:count,name).
    pub function_hits: HashMap<String, u64>,
    /// Line hit counts: line_number → execution count (from DA:line,count).
    pub line_hits: HashMap<usize, u64>,
}

/// Maximum number of comma-separated fields in a DA record (line,count,checksum).
const DA_MAX_FIELDS: usize = 3;

/// Parse and insert an FNDA record: "count,function_name".
/// Operation: string splitting + number parsing.
fn insert_fnda(data: &str, file_data: &mut LcovFileData) {
    data.split_once(',')
        .and_then(|(c, n)| c.parse::<u64>().ok().map(|count| (n, count)))
        .iter()
        .for_each(|(name, count)| {
            file_data.function_hits.insert(name.to_string(), *count);
        });
}

/// Parse and insert a DA record: "line_number,count[,checksum]".
/// Operation: string splitting + number parsing.
fn insert_da(data: &str, file_data: &mut LcovFileData) {
    let mut parts = data.splitn(DA_MAX_FIELDS, ',');
    let line = parts.next().and_then(|s| s.parse::<usize>().ok());
    let count = parts.next().and_then(|s| s.parse::<u64>().ok());
    line.zip(count)
        .iter()
        .for_each(|(l, c)| { file_data.line_hits.insert(*l, *c); });
}

/// Parse an LCOV file into per-file coverage data.
/// Operation: line-by-line parsing with state machine logic.
pub(crate) fn parse_lcov(path: &Path) -> Result<HashMap<String, LcovFileData>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read LCOV file {}: {e}", path.display()))?;

    let mut result: HashMap<String, LcovFileData> = HashMap::new();
    let mut current_file = String::new();
    let mut current_data = LcovFileData::default();

    content
        .lines()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .for_each(|trimmed| {
            if let Some(sf) = trimmed.strip_prefix("SF:") {
                current_file = sf.to_string();
                current_data = LcovFileData::default();
            } else if let Some(fnda) = trimmed.strip_prefix("FNDA:") {
                insert_fnda(fnda, &mut current_data);
            } else if let Some(da) = trimmed.strip_prefix("DA:") {
                insert_da(da, &mut current_data);
            } else if trimmed == "end_of_record" && !current_file.is_empty() {
                result.insert(
                    std::mem::take(&mut current_file),
                    std::mem::take(&mut current_data),
                );
            }
        });

    // Handle file without trailing end_of_record
    if !current_file.is_empty() {
        result.insert(current_file, current_data);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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
}
