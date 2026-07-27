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

/// The identifiers inside a mangled LCOV symbol.
///
/// `llvm-cov` writes Rust's v0 mangling (`_RNvCs…13sha256_digest`) or the
/// legacy form (`_ZN…17h<hash>E`); a plain name is returned unchanged. Both
/// encode the path as `<len><name>` segments, so the lengths are read rather
/// than guessed — splitting on digits would cut `sha256_digest` into `sha` and
/// `_digest`, and any name with a digit in it with them.
///
/// A length is only accepted when it yields a plausible identifier that ends
/// where the next segment or tag begins. That rejects the crate disambiguator
/// (`Cs569pcWMmiue_`), whose base-62 body puts digits where a length would be.
/// Operation: length-prefixed scan, no own calls.
pub(crate) fn symbol_base_names(symbol: &str) -> Vec<String> {
    if !symbol.starts_with("_R") && !symbol.starts_with("_ZN") {
        return vec![symbol.to_string()];
    }
    let chars: Vec<char> = symbol.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match read_segment(&chars, i) {
            Some((name, next)) => {
                out.push(name);
                i = next;
            }
            None => i += 1,
        }
    }
    out
}

/// One `<len><name>` segment at `at`, with the index after it. `None` when the
/// digits there are not a length — a disambiguator's base-62 body, or a count
/// that would run past the end or over something that is not an identifier.
/// Operation: bounds and shape checks, no own calls.
fn read_segment(chars: &[char], at: usize) -> Option<(String, usize)> {
    if !chars[at].is_ascii_digit() || chars[at] == '0' {
        return None;
    }
    let mut end = at;
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }
    let len: usize = chars[at..end].iter().collect::<String>().parse().ok()?;
    let stop = end.checked_add(len).filter(|s| *s <= chars.len())?;
    let name: String = chars[end..stop].iter().collect();
    let starts_ok = name.starts_with(|c: char| c.is_alphabetic() || c == '_');
    let body_ok = name.chars().all(|c| c.is_alphanumeric() || c == '_');
    // A segment ends where the next length or tag begins; anything else means
    // the digits were not a length.
    let ends_ok = chars
        .get(stop)
        .is_none_or(|c| c.is_ascii_digit() || c.is_ascii_uppercase());
    (starts_ok && body_ok && ends_ok).then_some((name, stop))
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
