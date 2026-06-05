//! AI output envelope helpers: group entries by file + project orphan
//! suppressions to JSON entries.

use serde_json::{json, Value};

const GLOBAL_FILE_KEY: &str = "<workspace>";

pub(super) fn group_by_file(entries: Vec<Value>) -> Value {
    let mut map = serde_json::Map::new();
    for mut entry in entries {
        let file_key = entry["file"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(GLOBAL_FILE_KEY)
            .to_string();
        if let Value::Object(ref mut o) = entry {
            o.remove("file");
        }
        let bucket = map
            .entry(file_key)
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Value::Array(arr) = bucket {
            arr.push(entry);
        }
    }
    Value::Object(map)
}

pub(super) fn orphan_suppression_entries(
    orphans: &[crate::domain::findings::OrphanSuppression],
) -> Vec<Value> {
    orphans
        .iter()
        .map(|w| {
            let dims: Vec<String> = w.dimensions.iter().map(|d| d.to_string()).collect();
            let scope = if dims.is_empty() {
                "<all>".to_string()
            } else {
                dims.join(",")
            };
            let suffix = w.target_suffix();
            let word = w.kind.status_word();
            let detail = match &w.reason {
                Some(r) => format!("{word} orphan suppression for {scope}{suffix} — {r}"),
                None => format!("{word} orphan suppression for {scope}{suffix}"),
            };
            json!({
                "file": w.file,
                "category": "orphan_suppression",
                "kind": w.kind.json_kind(),
                "line": w.line,
                "fn": "",
                "detail": detail,
            })
        })
        .collect()
}
