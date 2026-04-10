// qual:allow(srp) reason: "closely related reporting responsibilities; splitting not worthwhile"
use serde_json::{json, Value};

use crate::report::AnalysisResult;

/// Print analysis results in TOON format (Token-Oriented Object Notation).
/// Integration: builds AI value, encodes to TOON, prints.
pub fn print_ai(analysis: &AnalysisResult, config: &crate::config::Config) {
    let value = build_ai_value(analysis, config);
    print!("{}", encode_toon(&value, 0));
}

/// Print analysis results as compact AI-optimized JSON.
/// Integration: builds AI value, serializes to JSON, prints.
pub fn print_ai_json(analysis: &AnalysisResult, config: &crate::config::Config) {
    let value = build_ai_value(analysis, config);
    let json_str = serde_json::to_string(&value).unwrap_or_else(|_| format!("{value}"));
    println!("{json_str}");
}

/// Build the compact AI-optimized JSON value from analysis results.
/// Integration: orchestrates collect_all_findings + section builders via closures.
fn build_ai_value(analysis: &AnalysisResult, config: &crate::config::Config) -> Value {
    let findings = crate::report::findings_list::collect_all_findings(analysis);
    let total = findings.len();

    let mut obj = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "findings": total,
    });

    if total > 0 {
        let findings_value = build_findings_value(&findings, analysis, config);
        obj["findings_by_file"] = findings_value;
    }

    obj
}

/// Build findings grouped by file as a JSON object with enriched details.
/// Operation: sequential grouping + value construction, no own calls.
fn build_findings_value(
    entries: &[crate::report::findings_list::FindingEntry],
    analysis: &AnalysisResult,
    config: &crate::config::Config,
) -> Value {
    let mut map = serde_json::Map::new();
    let mut current_file = String::new();
    let mut current_entries: Vec<Value> = Vec::new();

    let file_key = |f: &str| {
        if f.is_empty() {
            GLOBAL_FILE_KEY.to_string()
        } else {
            f.to_string()
        }
    };
    entries.iter().for_each(|e| {
        let key = file_key(&e.file);
        if key != current_file {
            if !current_file.is_empty() {
                map.insert(
                    std::mem::take(&mut current_file),
                    Value::Array(std::mem::take(&mut current_entries)),
                );
            }
            current_file = key;
        }
        let cat = map_category(e.category);
        let detail = enrich_detail(e, analysis, config);
        current_entries.push(json!({
            "category": cat,
            "line": e.line,
            "fn": e.function_name,
            "detail": detail,
        }));
    });
    if !current_file.is_empty() {
        map.insert(current_file, Value::Array(current_entries));
    }

    Value::Object(map)
}

/// Enrich a finding's detail string with actionable context.
/// Operation: match on category + lookup in analysis data, no own calls.
fn enrich_detail(
    entry: &crate::report::findings_list::FindingEntry,
    analysis: &AnalysisResult,
    config: &crate::config::Config,
) -> String {
    let with_max = |threshold: usize| format!("{} (max {threshold})", entry.detail);
    match entry.category {
        "VIOLATION" => enrich_violation(entry, &analysis.results),
        "DUPLICATE" => enrich_partners(entry, &analysis.duplicates, "with"),
        "FRAGMENT" => enrich_partners(entry, &analysis.fragments, "also in"),
        "COGNITIVE" => with_max(config.complexity.max_cognitive),
        "CYCLOMATIC" => with_max(config.complexity.max_cyclomatic),
        "LONG_FN" => with_max(config.complexity.max_function_lines),
        "NESTING" => with_max(config.complexity.max_nesting_depth),
        "SRP_STRUCT" => enrich_srp_struct(entry, analysis),
        "SRP_MODULE" => with_max(config.srp.file_length_baseline),
        "SRP_PARAMS" => with_max(config.srp.max_parameters),
        _ => entry.detail.clone(),
    }
}

/// Enrich SRP struct detail with method and field counts.
/// Operation: search + format logic, no own calls.
fn enrich_srp_struct(
    entry: &crate::report::findings_list::FindingEntry,
    analysis: &AnalysisResult,
) -> String {
    let Some(srp) = &analysis.srp else {
        return entry.detail.clone();
    };
    let w = srp
        .struct_warnings
        .iter()
        .find(|w| w.file == entry.file && w.line == entry.line);
    let Some(w) = w else {
        return entry.detail.clone();
    };
    format!(
        "{}, {} methods, {} fields",
        entry.detail, w.method_count, w.field_count
    )
}

/// Enrich violation detail with logic and call line numbers.
/// Operation: search + format logic, no own calls.
fn enrich_violation(
    entry: &crate::report::findings_list::FindingEntry,
    results: &[crate::analyzer::FunctionAnalysis],
) -> String {
    let fa = results
        .iter()
        .find(|f| f.file == entry.file && f.line == entry.line);
    let Some(fa) = fa else {
        return entry.detail.clone();
    };
    if let crate::analyzer::Classification::Violation {
        logic_locations,
        call_locations,
        ..
    } = &fa.classification
    {
        let logic: Vec<String> = logic_locations.iter().map(|l| l.line.to_string()).collect();
        let calls: Vec<String> = call_locations.iter().map(|c| c.line.to_string()).collect();
        let mut parts = Vec::new();
        if !logic.is_empty() {
            parts.push(format!("logic lines {}", logic.join(",")));
        }
        if !calls.is_empty() {
            parts.push(format!("call lines {}", calls.join(",")));
        }
        if parts.is_empty() {
            entry.detail.clone()
        } else {
            parts.join("; ")
        }
    } else {
        entry.detail.clone()
    }
}

/// Trait for DRY finding groups that have partner locations.
trait HasPartnerLocations {
    fn partner_locations(&self, file: &str, line: usize) -> Vec<(String, usize)>;
    fn contains_entry(&self, file: &str, line: usize) -> bool;
}

impl HasPartnerLocations for crate::dry::functions::DuplicateGroup {
    fn partner_locations(&self, file: &str, line: usize) -> Vec<(String, usize)> {
        self.entries
            .iter()
            .filter(|e| !(e.file == file && e.line == line))
            .map(|e| (e.file.clone(), e.line))
            .collect()
    }
    fn contains_entry(&self, file: &str, line: usize) -> bool {
        self.entries
            .iter()
            .any(|e| e.file == file && e.line == line)
    }
}

impl HasPartnerLocations for crate::dry::fragments::FragmentGroup {
    fn partner_locations(&self, file: &str, line: usize) -> Vec<(String, usize)> {
        self.entries
            .iter()
            .filter(|e| !(e.file == file && e.start_line == line))
            .map(|e| (e.file.clone(), e.start_line))
            .collect()
    }
    fn contains_entry(&self, file: &str, line: usize) -> bool {
        self.entries
            .iter()
            .any(|e| e.file == file && e.start_line == line)
    }
}

/// Enrich detail with partner locations from grouped findings.
/// Operation: trait-based search + format logic, no own calls.
fn enrich_partners<G: HasPartnerLocations>(
    entry: &crate::report::findings_list::FindingEntry,
    groups: &[G],
    join_word: &str,
) -> String {
    let group = groups
        .iter()
        .find(|g| g.contains_entry(&entry.file, entry.line));
    let Some(group) = group else {
        return entry.detail.clone();
    };
    let partners: Vec<String> = group
        .partner_locations(&entry.file, entry.line)
        .iter()
        .map(|(f, l)| format!("{f}:{l}"))
        .collect();
    if partners.is_empty() {
        return entry.detail.clone();
    }
    format!("{} {join_word} {}", entry.detail, partners.join(", "))
}

// ── Minimal TOON encoder ────────────────────────────────────

/// Key used for findings without a file location (e.g., coupling, cycles, SDP).
const GLOBAL_FILE_KEY: &str = "<global>";
const INDENT: &str = "  ";
const TOON_SPECIAL: &[char] = &[',', ':', '"', '\\', '[', ']', '{', '}'];

/// Encode a serde_json::Value as TOON string.
/// Operation: recursive match on Value variants, no own calls (recursive via closure pattern).
fn encode_toon(value: &Value, depth: usize) -> String {
    let indent = INDENT.repeat(depth);
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => toon_quote(s),
        Value::Array(arr) if is_tabular(arr) => encode_tabular(arr, depth),
        Value::Array(arr) => encode_list(arr, depth),
        Value::Object(obj) => {
            let mut lines = Vec::new();
            obj.iter().for_each(|(k, v)| match v {
                Value::Object(_) | Value::Array(_) => {
                    lines.push(format!("{indent}{}:", toon_quote(k)));
                    let child = encode_toon(v, depth + 1);
                    lines.push(child);
                }
                _ => lines.push(format!("{indent}{}: {}", toon_quote(k), encode_toon(v, 0))),
            });
            lines.join("\n")
        }
    }
}

/// Check if an array is tabular (all elements are objects with identical key sets).
/// Operation: comparison logic, no own calls.
fn is_tabular(arr: &[Value]) -> bool {
    if arr.len() < 2 {
        return false;
    }
    let Some(Value::Object(first)) = arr.first() else {
        return false;
    };
    let keys: Vec<&String> = first.keys().collect();
    arr[1..].iter().all(|v| {
        v.as_object()
            .map(|o| {
                o.len() == keys.len()
                    && keys.iter().all(|k| o.contains_key(k.as_str()))
                    && o.values().all(|v| !v.is_object() && !v.is_array())
            })
            .unwrap_or(false)
    })
}

/// Encode a tabular array as TOON with header row.
/// Operation: formatting logic, no own calls.
fn encode_tabular(arr: &[Value], depth: usize) -> String {
    let indent = INDENT.repeat(depth);
    let row_indent = INDENT.repeat(depth + 1);
    let Some(first) = arr[0].as_object() else {
        return String::new();
    };
    let fields: Vec<&String> = first.keys().collect();
    let header = fields
        .iter()
        .map(|f| f.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let mut lines = vec![format!("{indent}[{}]{{{header}}}:", arr.len())];
    arr.iter().for_each(|row| {
        let Some(obj) = row.as_object() else { return };
        let vals: Vec<String> = fields
            .iter()
            .map(|f| encode_toon(&obj[f.as_str()], 0))
            .collect();
        lines.push(format!("{row_indent}{}", vals.join(",")));
    });
    lines.join("\n")
}

/// Encode a non-tabular array as TOON list.
/// Operation: formatting logic, no own calls.
fn encode_list(arr: &[Value], depth: usize) -> String {
    let row_indent = INDENT.repeat(depth + 1);
    let mut lines = Vec::new();
    arr.iter().for_each(|v| {
        lines.push(format!("{row_indent}- {}", encode_toon(v, 0)));
    });
    lines.join("\n")
}

/// Quote a string if it contains TOON special characters or starts with `-`.
/// Operation: char scan + escape logic, no own calls.
fn toon_quote(s: &str) -> String {
    if s.is_empty() || s.starts_with('-') || s.contains(TOON_SPECIAL) {
        let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

/// Map FindingEntry.category to human-readable snake_case for AI output.
/// Operation: match expression, no own calls.
fn map_category(cat: &str) -> &str {
    match cat {
        "VIOLATION" => "violation",
        "COGNITIVE" => "cognitive_complexity",
        "CYCLOMATIC" => "cyclomatic_complexity",
        "MAGIC_NUMBER" => "magic_number",
        "NESTING" => "nesting_depth",
        "LONG_FN" => "long_function",
        "UNSAFE" => "unsafe_block",
        "ERROR_HANDLING" => "error_handling",
        "DUPLICATE" => "duplicate",
        "DEAD_CODE" => "dead_code",
        "FRAGMENT" => "fragment",
        "BOILERPLATE" => "boilerplate",
        "WILDCARD" => "wildcard_import",
        "REPEATED_MATCH" => "repeated_match",
        "SRP_STRUCT" => "srp_struct",
        "SRP_MODULE" => "srp_module",
        "SRP_PARAMS" => "srp_params",
        "COUPLING" => "coupling",
        "CYCLE" => "cycle",
        "SDP" => "sdp_violation",
        "TQ_NO_ASSERT" => "no_assertion",
        "TQ_NO_SUT" => "no_sut_call",
        "TQ_UNTESTED" => "untested",
        "TQ_UNCOVERED" => "uncovered",
        "TQ_UNTESTED_LOGIC" => "untested_logic",
        "STRUCTURAL" => "structural",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── TOON encoder tests ──────────────────────────────────────

    #[test]
    fn test_toon_quote_plain() {
        assert_eq!(toon_quote("hello"), "hello");
        assert_eq!(toon_quote("foo_bar"), "foo_bar");
    }

    #[test]
    fn test_toon_quote_special_chars() {
        assert_eq!(toon_quote("a,b"), "\"a,b\"");
        assert_eq!(toon_quote("key: val"), "\"key: val\"");
        assert_eq!(toon_quote(""), "\"\"");
    }

    #[test]
    fn test_toon_quote_dash_start() {
        assert_eq!(toon_quote("-flag"), "\"-flag\"");
    }

    #[test]
    fn test_toon_quote_escapes() {
        assert_eq!(toon_quote("say \"hi\""), "\"say \\\"hi\\\"\"");
        assert_eq!(toon_quote("a\\b"), "\"a\\\\b\"");
    }

    #[test]
    fn test_encode_toon_primitives() {
        assert_eq!(encode_toon(&json!(null), 0), "null");
        assert_eq!(encode_toon(&json!(true), 0), "true");
        assert_eq!(encode_toon(&json!(42), 0), "42");
        assert_eq!(encode_toon(&json!("hello"), 0), "hello");
        assert_eq!(encode_toon(&json!("a,b"), 0), "\"a,b\"");
    }

    #[test]
    fn test_encode_toon_flat_object() {
        let val = json!({"version": "0.5.5", "findings": 0});
        let toon = encode_toon(&val, 0);
        assert!(toon.contains("version: 0.5.5"), "got: {toon}");
        assert!(toon.contains("findings: 0"), "got: {toon}");
    }

    #[test]
    fn test_encode_toon_tabular_array() {
        let val = json!([
            {"name": "IOSP", "pct": 100.0},
            {"name": "CX", "pct": 99.8},
        ]);
        let toon = encode_toon(&val, 0);
        assert!(
            toon.contains("[2]{name,pct}:"),
            "should have tabular header, got: {toon}"
        );
        assert!(toon.contains("IOSP,100.0"), "got: {toon}");
        assert!(toon.contains("CX,99.8"), "got: {toon}");
    }

    #[test]
    fn test_encode_toon_non_tabular_array() {
        let val = json!(["a", "b", "c"]);
        let toon = encode_toon(&val, 0);
        assert!(toon.contains("- a"), "got: {toon}");
        assert!(toon.contains("- b"), "got: {toon}");
    }

    #[test]
    fn test_is_tabular_uniform_objects() {
        let arr = vec![json!({"a": 1, "b": 2}), json!({"a": 3, "b": 4})];
        assert!(is_tabular(&arr));
    }

    #[test]
    fn test_is_tabular_rejects_mixed() {
        assert!(!is_tabular(&[json!(1), json!(2)]));
        assert!(!is_tabular(&[json!({"a": 1}), json!({"b": 2})]));
        assert!(!is_tabular(&[json!({"a": [1]}), json!({"a": [2]})]));
        assert!(!is_tabular(&[json!({"a": 1})]));
    }

    // ── Category mapping tests ──────────────────────────────────

    #[test]
    fn test_map_category_all_known() {
        let cases = [
            ("VIOLATION", "violation"),
            ("COGNITIVE", "cognitive_complexity"),
            ("CYCLOMATIC", "cyclomatic_complexity"),
            ("MAGIC_NUMBER", "magic_number"),
            ("NESTING", "nesting_depth"),
            ("LONG_FN", "long_function"),
            ("UNSAFE", "unsafe_block"),
            ("ERROR_HANDLING", "error_handling"),
            ("DUPLICATE", "duplicate"),
            ("DEAD_CODE", "dead_code"),
            ("FRAGMENT", "fragment"),
            ("BOILERPLATE", "boilerplate"),
            ("WILDCARD", "wildcard_import"),
            ("REPEATED_MATCH", "repeated_match"),
            ("SRP_STRUCT", "srp_struct"),
            ("SRP_MODULE", "srp_module"),
            ("SRP_PARAMS", "srp_params"),
            ("COUPLING", "coupling"),
            ("CYCLE", "cycle"),
            ("SDP", "sdp_violation"),
            ("TQ_NO_ASSERT", "no_assertion"),
            ("TQ_NO_SUT", "no_sut_call"),
            ("TQ_UNTESTED", "untested"),
            ("TQ_UNCOVERED", "uncovered"),
            ("TQ_UNTESTED_LOGIC", "untested_logic"),
            ("STRUCTURAL", "structural"),
        ];
        cases.iter().for_each(|(input, expected)| {
            assert_eq!(
                map_category(input),
                *expected,
                "map_category({input}) should return {expected}"
            );
        });
    }

    fn empty_analysis() -> AnalysisResult {
        AnalysisResult {
            results: vec![],
            summary: crate::report::Summary::default(),
            coupling: None,
            duplicates: vec![],
            dead_code: vec![],
            fragments: vec![],
            boilerplate: vec![],
            wildcard_warnings: vec![],
            repeated_matches: vec![],
            srp: None,
            tq: None,
            structural: None,
        }
    }

    #[test]
    fn test_build_ai_value_zero_findings() {
        let analysis = empty_analysis();
        let config = crate::config::Config::default();
        let value = build_ai_value(&analysis, &config);

        assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(value["findings"], 0);
        assert!(
            value.get("findings_by_file").is_none(),
            "no findings_by_file when 0 findings"
        );
    }

    #[test]
    fn test_build_findings_grouped_by_file() {
        use crate::report::findings_list::FindingEntry;

        let entries = vec![
            FindingEntry {
                file: "src/a.rs".into(),
                line: 10,
                category: "MAGIC_NUMBER",
                detail: "42".into(),
                function_name: "fn_a".into(),
            },
            FindingEntry {
                file: "src/a.rs".into(),
                line: 20,
                category: "LONG_FN",
                detail: "72 lines".into(),
                function_name: "fn_b".into(),
            },
            FindingEntry {
                file: "src/b.rs".into(),
                line: 5,
                category: "DUPLICATE",
                detail: "exact".into(),
                function_name: "fn_c".into(),
            },
        ];

        let analysis = empty_analysis();
        let config = crate::config::Config::default();
        let value = build_findings_value(&entries, &analysis, &config);
        let obj = value.as_object().unwrap();

        // Two file groups
        assert_eq!(obj.len(), 2, "should group into 2 files");
        assert!(obj.contains_key("src/a.rs"));
        assert!(obj.contains_key("src/b.rs"));

        // src/a.rs has 2 entries
        let a_entries = obj["src/a.rs"].as_array().unwrap();
        assert_eq!(a_entries.len(), 2);
        assert_eq!(a_entries[0]["category"], "magic_number");
        assert_eq!(a_entries[0]["line"], 10);
        assert_eq!(a_entries[0]["fn"], "fn_a");
        assert_eq!(a_entries[0]["detail"], "42");
        assert_eq!(a_entries[1]["category"], "long_function");
        assert_eq!(a_entries[1]["line"], 20);

        // src/b.rs has 1 entry
        let b_entries = obj["src/b.rs"].as_array().unwrap();
        assert_eq!(b_entries.len(), 1);
        assert_eq!(b_entries[0]["category"], "duplicate");
        assert_eq!(b_entries[0]["fn"], "fn_c");
    }

    #[test]
    fn test_enrich_violation_detail() {
        use crate::analyzer::{CallOccurrence, Classification, FunctionAnalysis, LogicOccurrence};
        use crate::report::findings_list::FindingEntry;

        let mut analysis = empty_analysis();
        analysis.results = vec![FunctionAnalysis {
            name: "bad_fn".into(),
            file: "src/lib.rs".into(),
            line: 40,
            classification: Classification::Violation {
                has_logic: true,
                has_own_calls: true,
                logic_locations: vec![
                    LogicOccurrence {
                        line: 44,
                        kind: "if".into(),
                    },
                    LogicOccurrence {
                        line: 47,
                        kind: "for".into(),
                    },
                ],
                call_locations: vec![CallOccurrence {
                    line: 50,
                    name: "helper".into(),
                }],
            },
            parent_type: None,
            suppressed: false,
            complexity: None,
            qualified_name: "bad_fn".into(),
            severity: None,
            cognitive_warning: false,
            cyclomatic_warning: false,
            nesting_depth_warning: false,
            function_length_warning: false,
            unsafe_warning: false,
            error_handling_warning: false,
            complexity_suppressed: false,
            own_calls: vec![],
            parameter_count: 0,
            is_trait_impl: false,
            is_test: false,
            effort_score: None,
        }];

        let entries = vec![FindingEntry {
            file: "src/lib.rs".into(),
            line: 40,
            category: "VIOLATION",
            detail: "logic + calls".into(),
            function_name: "bad_fn".into(),
        }];

        let config = crate::config::Config::default();
        let value = build_findings_value(&entries, &analysis, &config);
        let arr = value["src/lib.rs"].as_array().unwrap();
        let detail = arr[0]["detail"].as_str().unwrap();
        assert!(
            detail.contains("logic lines 44,47"),
            "detail should show logic lines, got: {detail}"
        );
        assert!(
            detail.contains("call lines 50"),
            "detail should show call lines, got: {detail}"
        );
    }

    #[test]
    fn test_enrich_duplicate_detail() {
        use crate::dry::functions::{DuplicateEntry, DuplicateGroup, DuplicateKind};
        use crate::report::findings_list::FindingEntry;

        let mut analysis = empty_analysis();
        analysis.duplicates = vec![DuplicateGroup {
            entries: vec![
                DuplicateEntry {
                    name: "fn_a".into(),
                    qualified_name: "mod::fn_a".into(),
                    file: "src/a.rs".into(),
                    line: 10,
                },
                DuplicateEntry {
                    name: "fn_b".into(),
                    qualified_name: "mod::fn_b".into(),
                    file: "src/b.rs".into(),
                    line: 20,
                },
            ],
            kind: DuplicateKind::Exact,
            suppressed: false,
        }];

        let entries = vec![
            FindingEntry {
                file: "src/a.rs".into(),
                line: 10,
                category: "DUPLICATE",
                detail: "exact".into(),
                function_name: "mod::fn_a".into(),
            },
            FindingEntry {
                file: "src/b.rs".into(),
                line: 20,
                category: "DUPLICATE",
                detail: "exact".into(),
                function_name: "mod::fn_b".into(),
            },
        ];

        let config = crate::config::Config::default();
        let value = build_findings_value(&entries, &analysis, &config);
        let a_detail = value["src/a.rs"].as_array().unwrap()[0]["detail"]
            .as_str()
            .unwrap();
        let b_detail = value["src/b.rs"].as_array().unwrap()[0]["detail"]
            .as_str()
            .unwrap();
        assert!(
            a_detail.contains("src/b.rs:20"),
            "should reference partner location, got: {a_detail}"
        );
        assert!(
            b_detail.contains("src/a.rs:10"),
            "should reference partner location, got: {b_detail}"
        );
    }

    #[test]
    fn test_enrich_fragment_detail() {
        use crate::dry::fragments::{FragmentEntry, FragmentGroup};
        use crate::report::findings_list::FindingEntry;

        let mut analysis = empty_analysis();
        analysis.fragments = vec![FragmentGroup {
            entries: vec![
                FragmentEntry {
                    function_name: "fn_a".into(),
                    qualified_name: "fn_a".into(),
                    file: "src/a.rs".into(),
                    start_line: 10,
                    end_line: 15,
                },
                FragmentEntry {
                    function_name: "fn_b".into(),
                    qualified_name: "fn_b".into(),
                    file: "src/b.rs".into(),
                    start_line: 30,
                    end_line: 35,
                },
            ],
            statement_count: 3,
            suppressed: false,
        }];

        let entries = vec![
            FindingEntry {
                file: "src/a.rs".into(),
                line: 10,
                category: "FRAGMENT",
                detail: "3 stmts".into(),
                function_name: "fn_a".into(),
            },
            FindingEntry {
                file: "src/b.rs".into(),
                line: 30,
                category: "FRAGMENT",
                detail: "3 stmts".into(),
                function_name: "fn_b".into(),
            },
        ];

        let config = crate::config::Config::default();
        let value = build_findings_value(&entries, &analysis, &config);
        let a_detail = value["src/a.rs"].as_array().unwrap()[0]["detail"]
            .as_str()
            .unwrap();
        assert!(
            a_detail.contains("also in src/b.rs:30"),
            "should reference partner, got: {a_detail}"
        );
    }

    #[test]
    fn test_global_findings_not_dropped() {
        use crate::report::findings_list::FindingEntry;
        let entries = vec![
            FindingEntry {
                file: "".into(),
                line: 0,
                category: "COUPLING",
                detail: "I=0.71 Ca=2 Ce=5".into(),
                function_name: "db".into(),
            },
            FindingEntry {
                file: "src/a.rs".into(),
                line: 10,
                category: "MAGIC_NUMBER",
                detail: "42".into(),
                function_name: "fn_a".into(),
            },
        ];
        let analysis = empty_analysis();
        let config = crate::config::Config::default();
        let value = build_findings_value(&entries, &analysis, &config);
        let obj = value.as_object().unwrap();
        assert!(
            obj.contains_key(GLOBAL_FILE_KEY),
            "empty-file findings should be under GLOBAL_FILE_KEY"
        );
        assert!(obj.contains_key("src/a.rs"));
        let global = obj[GLOBAL_FILE_KEY].as_array().unwrap();
        assert_eq!(global.len(), 1);
        assert_eq!(global[0]["category"], "coupling");
    }

    #[test]
    fn test_enrich_complexity_detail() {
        use crate::report::findings_list::FindingEntry;
        let analysis = empty_analysis();
        let entry = FindingEntry {
            file: "src/lib.rs".into(),
            line: 10,
            category: "COGNITIVE",
            detail: "complexity 12".into(),
            function_name: "fn1".into(),
        };
        let config = crate::config::Config::default();
        let detail = enrich_detail(&entry, &analysis, &config);
        assert!(detail.contains("12"), "should contain value, got: {detail}");
        assert!(
            detail.contains(&format!("max {}", config.complexity.max_cognitive)),
            "should contain threshold, got: {detail}"
        );
    }

    #[test]
    fn test_enrich_long_function_detail() {
        use crate::report::findings_list::FindingEntry;
        let analysis = empty_analysis();
        let entry = FindingEntry {
            file: "src/lib.rs".into(),
            line: 10,
            category: "LONG_FN",
            detail: "72 lines".into(),
            function_name: "fn1".into(),
        };
        let config = crate::config::Config::default();
        let detail = enrich_detail(&entry, &analysis, &config);
        assert!(
            detail.contains("72 lines"),
            "should contain line count, got: {detail}"
        );
        assert!(
            detail.contains(&format!("max {}", config.complexity.max_function_lines)),
            "should contain threshold, got: {detail}"
        );
    }

    #[test]
    fn test_enrich_nesting_detail() {
        use crate::report::findings_list::FindingEntry;
        let analysis = empty_analysis();
        let entry = FindingEntry {
            file: "src/lib.rs".into(),
            line: 10,
            category: "NESTING",
            detail: "depth 5".into(),
            function_name: "fn1".into(),
        };
        let config = crate::config::Config::default();
        let detail = enrich_detail(&entry, &analysis, &config);
        assert!(
            detail.contains("depth 5"),
            "should contain depth, got: {detail}"
        );
        assert!(
            detail.contains(&format!("max {}", config.complexity.max_nesting_depth)),
            "should contain threshold, got: {detail}"
        );
    }

    #[test]
    fn test_enrich_srp_struct_detail() {
        use crate::report::findings_list::FindingEntry;
        use crate::srp::{SrpAnalysis, SrpWarning};
        let mut analysis = empty_analysis();
        analysis.srp = Some(SrpAnalysis {
            struct_warnings: vec![SrpWarning {
                struct_name: "BigStruct".into(),
                file: "src/lib.rs".into(),
                line: 10,
                lcom4: 3,
                field_count: 8,
                method_count: 12,
                fan_out: 5,
                composite_score: 0.85,
                clusters: vec![],
                suppressed: false,
            }],
            module_warnings: vec![],
            param_warnings: vec![],
        });
        let entry = FindingEntry {
            file: "src/lib.rs".into(),
            line: 10,
            category: "SRP_STRUCT",
            detail: "LCOM4=3".into(),
            function_name: "BigStruct".into(),
        };
        let config = crate::config::Config::default();
        let detail = enrich_detail(&entry, &analysis, &config);
        assert!(
            detail.contains("LCOM4=3"),
            "should contain LCOM4, got: {detail}"
        );
        assert!(
            detail.contains("12 methods"),
            "should contain method count, got: {detail}"
        );
        assert!(
            detail.contains("8 fields"),
            "should contain field count, got: {detail}"
        );
    }

    #[test]
    fn test_enrich_srp_module_detail() {
        use crate::report::findings_list::FindingEntry;
        let analysis = empty_analysis();
        let entry = FindingEntry {
            file: "src/lib.rs".into(),
            line: 1,
            category: "SRP_MODULE",
            detail: "310 lines".into(),
            function_name: "lib".into(),
        };
        let config = crate::config::Config::default();
        let detail = enrich_detail(&entry, &analysis, &config);
        assert!(
            detail.contains("310 lines"),
            "should contain line count, got: {detail}"
        );
        assert!(
            detail.contains(&format!("max {}", config.srp.file_length_baseline)),
            "should contain threshold, got: {detail}"
        );
    }

    #[test]
    fn test_enrich_srp_params_detail() {
        use crate::report::findings_list::FindingEntry;
        let analysis = empty_analysis();
        let entry = FindingEntry {
            file: "src/lib.rs".into(),
            line: 10,
            category: "SRP_PARAMS",
            detail: "7 params".into(),
            function_name: "fn1".into(),
        };
        let config = crate::config::Config::default();
        let detail = enrich_detail(&entry, &analysis, &config);
        assert!(
            detail.contains("7 params"),
            "should contain param count, got: {detail}"
        );
        assert!(
            detail.contains(&format!("max {}", config.srp.max_parameters)),
            "should contain threshold, got: {detail}"
        );
    }

    #[test]
    fn test_build_findings_empty() {
        let analysis = empty_analysis();
        let config = crate::config::Config::default();
        let value = build_findings_value(&[], &analysis, &config);
        assert!(value.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_build_ai_value_with_findings() {
        use crate::analyzer::{
            Classification, ComplexityMetrics, FunctionAnalysis, MagicNumberOccurrence,
        };

        let mut analysis = empty_analysis();
        let fa = FunctionAnalysis {
            name: "fn1".into(),
            file: "src/lib.rs".into(),
            line: 10,
            classification: Classification::Operation,
            parent_type: None,
            suppressed: false,
            complexity: Some(ComplexityMetrics {
                magic_numbers: vec![MagicNumberOccurrence {
                    line: 12,
                    value: "42".into(),
                }],
                ..Default::default()
            }),
            qualified_name: "fn1".into(),
            severity: None,
            cognitive_warning: false,
            cyclomatic_warning: false,
            nesting_depth_warning: false,
            function_length_warning: false,
            unsafe_warning: false,
            error_handling_warning: false,
            complexity_suppressed: false,
            own_calls: vec![],
            parameter_count: 0,
            is_trait_impl: false,
            is_test: false,
            effort_score: None,
        };
        analysis.results = vec![fa];

        let config = crate::config::Config::default();
        let value = build_ai_value(&analysis, &config);
        assert_eq!(value["findings"], 1);
        assert!(
            value.get("findings_by_file").is_some(),
            "should have findings_by_file"
        );

        let by_file = value["findings_by_file"].as_object().unwrap();
        assert!(by_file.contains_key("src/lib.rs"));
        let entries = by_file["src/lib.rs"].as_array().unwrap();
        assert_eq!(entries[0]["category"], "magic_number");
        assert_eq!(entries[0]["line"], 12);
    }

    #[test]
    fn test_toon_output_contains_version_and_findings() {
        let analysis = empty_analysis();
        let config = crate::config::Config::default();
        let value = build_ai_value(&analysis, &config);
        let toon = encode_toon(&value, 0);
        assert!(toon.contains("version:"), "TOON should contain version key");
        assert!(toon.contains("findings: 0"), "TOON should show 0 findings");
        assert!(
            !toon.contains("findings_by_file"),
            "TOON should not have findings_by_file when 0"
        );
    }

    #[test]
    fn test_ai_json_output_parseable() {
        let analysis = empty_analysis();
        let config = crate::config::Config::default();
        let value = build_ai_value(&analysis, &config);
        let json_str = serde_json::to_string_pretty(&value).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(parsed["findings"], 0);
    }

    #[test]
    fn test_toon_output_with_findings_has_tabular_format() {
        use crate::analyzer::{
            Classification, ComplexityMetrics, FunctionAnalysis, MagicNumberOccurrence,
        };

        let mut analysis = empty_analysis();
        analysis.results = vec![FunctionAnalysis {
            name: "fn1".into(),
            file: "src/lib.rs".into(),
            line: 10,
            classification: Classification::Operation,
            parent_type: None,
            suppressed: false,
            complexity: Some(ComplexityMetrics {
                magic_numbers: vec![
                    MagicNumberOccurrence {
                        line: 12,
                        value: "42".into(),
                    },
                    MagicNumberOccurrence {
                        line: 15,
                        value: "99".into(),
                    },
                ],
                ..Default::default()
            }),
            qualified_name: "fn1".into(),
            severity: None,
            cognitive_warning: false,
            cyclomatic_warning: false,
            nesting_depth_warning: false,
            function_length_warning: false,
            unsafe_warning: false,
            error_handling_warning: false,
            complexity_suppressed: false,
            own_calls: vec![],
            parameter_count: 0,
            is_trait_impl: false,
            is_test: false,
            effort_score: None,
        }];

        let config = crate::config::Config::default();
        let value = build_ai_value(&analysis, &config);
        let toon = encode_toon(&value, 0);
        assert!(toon.contains("findings: 2"), "should show 2 findings");
        assert!(
            toon.contains("findings_by_file:"),
            "should have findings_by_file section"
        );
        // TOON tabular format: file name as key with [N]{fields}: header
        assert!(toon.contains("src/lib.rs"), "should contain file name");
        assert!(
            toon.contains("magic_number"),
            "should contain mapped category"
        );
    }

    #[test]
    fn test_map_category_unknown_passthrough() {
        assert_eq!(map_category("UNKNOWN_CAT"), "UNKNOWN_CAT");
        assert_eq!(map_category("NEW_FINDING"), "NEW_FINDING");
    }
}
