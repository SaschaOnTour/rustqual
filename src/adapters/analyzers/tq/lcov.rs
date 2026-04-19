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
    line.zip(count).iter().for_each(|(l, c)| {
        file_data.line_hits.insert(*l, *c);
    });
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
