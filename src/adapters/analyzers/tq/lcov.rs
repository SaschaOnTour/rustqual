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

/// Execution counts keyed by function name rather than by mangled symbol.
///
/// One entry per monomorphisation becomes one entry per function: if any
/// instantiation ran, the function ran. Every identifier a symbol yields gets
/// the count, so a module or crate name collects hits too — harmless, since
/// only declared function names are ever looked up, and an inflated count can
/// only suppress a finding.
/// Operation: fold over the raw symbols, own call in the closure.
pub(crate) fn hits_by_function_name(data: &LcovFileData) -> HashMap<String, u64> {
    let mut out: HashMap<String, u64> = HashMap::new();
    data.function_hits.iter().for_each(|(symbol, hits)| {
        symbol_base_names(symbol).into_iter().for_each(|name| {
            *out.entry(name).or_insert(0) += hits;
        });
    });
    out
}

/// The identifiers inside a mangled LCOV symbol.
///
/// `llvm-cov` writes Rust's v0 mangling (`_RNvNtNtCs…20capture_secret_event`)
/// or the legacy form (`_ZN…17h<hash>E`); a plain name is returned unchanged.
/// Both encode the path as length-prefixed segments, but the crate
/// disambiguator (`Cs569pcWMmiue_`) puts digits where a length would be, so the
/// segments are read by splitting on digits rather than by trusting the counts.
///
/// Every run is yielded, not just the last: a symbol for a closure or a trait
/// impl *inside* a function carries the function's name in the middle
/// (`…capture_secret_event…BufWriter…flush`), and the outer name is the one
/// that matters. Crate names, module names and mangling fragments come along —
/// over-collection, which for the tested set only suppresses a finding.
/// Operation: split on digits, no own calls.
pub(crate) fn symbol_base_names(symbol: &str) -> Vec<String> {
    if !symbol.starts_with("_R") && !symbol.starts_with("_ZN") {
        return vec![symbol.to_string()];
    }
    symbol
        .split(|c: char| c.is_ascii_digit())
        .filter(|run| !run.is_empty())
        .flat_map(|run| [run.to_string(), snake_prefix(run)])
        .filter(|name| !name.is_empty())
        .collect()
}

/// The leading snake_case part of a mangled run.
///
/// A monomorphised symbol runs the function name straight into the type
/// arguments — `append_ticksNtNtCs…SqliteStorage` — so splitting on digits
/// alone yields `append_ticksNtNtCs`. Rust function names are snake_case and
/// the mangling appends CamelCase tags, so the first uppercase letter is the
/// boundary. Emitted alongside the full run, never instead of it.
/// Operation: prefix scan, no own calls.
fn snake_prefix(run: &str) -> String {
    run.chars()
        .take_while(|c| !c.is_ascii_uppercase())
        .collect()
}
