// qual:allow(srp) reason: "closely related reporting responsibilities; splitting not worthwhile"
use serde_json::{json, Value};

use crate::report::AnalysisResult;

/// Print analysis results in TOON format (Token-Oriented Object Notation).
/// Integration: builds AI value, encodes to TOON, prints.
pub fn print_ai(analysis: &AnalysisResult, config: &crate::config::Config) {
    let value = build_ai_value(analysis, config);
    println!("{}", toon_encode::encode_toon(&value, 0));
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
pub(crate) fn build_ai_value(analysis: &AnalysisResult, config: &crate::config::Config) -> Value {
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

/// Pre-built indexes for O(1) enrichment lookups.
pub(crate) struct EnrichIndex<'a> {
    results: std::collections::HashMap<
        (&'a str, usize),
        &'a crate::adapters::analyzers::iosp::FunctionAnalysis,
    >,
    duplicates: std::collections::HashMap<
        (&'a str, usize),
        &'a crate::adapters::analyzers::dry::functions::DuplicateGroup,
    >,
    fragments: std::collections::HashMap<
        (&'a str, usize),
        &'a crate::adapters::analyzers::dry::fragments::FragmentGroup,
    >,
    srp_structs: std::collections::HashMap<
        (&'a str, usize),
        &'a crate::adapters::analyzers::srp::SrpWarning,
    >,
    // SRP module warnings are emitted as findings at line 1 per file,
    // so file alone is the natural key.
    srp_modules:
        std::collections::HashMap<&'a str, &'a crate::adapters::analyzers::srp::ModuleSrpWarning>,
    // Global findings (file empty, line 0) need a different key: use
    // the detail string shape the collector emits.
    sdp: std::collections::HashMap<
        String,
        &'a crate::adapters::analyzers::coupling::sdp::SdpViolation,
    >,
    boilerplate: std::collections::HashMap<
        (&'a str, usize),
        &'a crate::adapters::analyzers::dry::boilerplate::BoilerplateFind,
    >,
    dead_code: std::collections::HashMap<
        (&'a str, usize),
        &'a crate::adapters::analyzers::dry::dead_code::DeadCodeWarning,
    >,
    structural: std::collections::HashMap<
        (&'a str, usize),
        &'a crate::adapters::analyzers::structural::StructuralWarning,
    >,
    orphan_suppressions: std::collections::HashMap<
        (&'a str, usize),
        &'a crate::adapters::report::OrphanSuppressionWarning,
    >,
}

/// Build enrichment indexes from analysis data for O(1) lookups.
/// Operation: iteration + HashMap construction, no own calls.
pub(crate) fn build_enrich_index(analysis: &AnalysisResult) -> EnrichIndex<'_> {
    let results = analysis
        .results
        .iter()
        .map(|fa| ((fa.file.as_str(), fa.line), fa))
        .collect();
    let duplicates = analysis
        .duplicates
        .iter()
        .flat_map(|g| {
            g.entries
                .iter()
                .map(move |e| ((e.file.as_str(), e.line), g))
        })
        .collect();
    let fragments = analysis
        .fragments
        .iter()
        .flat_map(|g| {
            g.entries
                .iter()
                .map(move |e| ((e.file.as_str(), e.start_line), g))
        })
        .collect();
    let srp_structs = analysis
        .srp
        .as_ref()
        .map(|s| {
            s.struct_warnings
                .iter()
                .map(|w| ((w.file.as_str(), w.line), w))
                .collect()
        })
        .unwrap_or_default();
    let srp_modules = analysis
        .srp
        .as_ref()
        .map(|s| {
            s.module_warnings
                .iter()
                .map(|w| (w.file.as_str(), w))
                .collect()
        })
        .unwrap_or_default();
    let sdp = analysis
        .coupling
        .as_ref()
        .map(|ca| {
            ca.sdp_violations
                .iter()
                .map(|v| (format!("{} -> {}", v.from_module, v.to_module), v))
                .collect()
        })
        .unwrap_or_default();
    let boilerplate = analysis
        .boilerplate
        .iter()
        .map(|b| ((b.file.as_str(), b.line), b))
        .collect();
    let dead_code = analysis
        .dead_code
        .iter()
        .map(|w| ((w.file.as_str(), w.line), w))
        .collect();
    let structural = analysis
        .structural
        .as_ref()
        .map(|s| {
            s.warnings
                .iter()
                .map(|w| ((w.file.as_str(), w.line), w))
                .collect()
        })
        .unwrap_or_default();
    let orphan_suppressions = analysis
        .orphan_suppressions
        .iter()
        .map(|w| ((w.file.as_str(), w.line), w))
        .collect();
    EnrichIndex {
        results,
        duplicates,
        fragments,
        srp_structs,
        srp_modules,
        sdp,
        boilerplate,
        dead_code,
        structural,
        orphan_suppressions,
    }
}

/// Build findings grouped by file as a JSON object with enriched details.
/// Operation: sequential grouping + value construction, no own calls.
pub(crate) fn build_findings_value(
    entries: &[crate::report::findings_list::FindingEntry],
    analysis: &AnalysisResult,
    config: &crate::config::Config,
) -> Value {
    let index = build_enrich_index(analysis);
    let mut map = serde_json::Map::new();
    let mut current_file = String::new();
    let mut current_entries: Vec<Value> = Vec::new();

    entries.iter().for_each(|e| {
        let key: &str = if e.file.is_empty() {
            GLOBAL_FILE_KEY
        } else {
            &e.file
        };
        if key != current_file {
            if !current_file.is_empty() {
                map.insert(
                    std::mem::take(&mut current_file),
                    Value::Array(std::mem::take(&mut current_entries)),
                );
            }
            current_file = key.to_string();
        }
        let cat = map_category(e.category);
        let detail = enrich_detail(e, &index, config);
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
/// Operation: match on category + O(1) index lookup, no own calls.
pub(crate) fn enrich_detail(
    entry: &crate::report::findings_list::FindingEntry,
    index: &EnrichIndex<'_>,
    config: &crate::config::Config,
) -> String {
    let with_max = |threshold: usize| format!("{} (max {threshold})", entry.detail);
    let key = (entry.file.as_str(), entry.line);
    match entry.category {
        "VIOLATION" => enrich_violation(entry, index.results.get(&key).copied()),
        "DUPLICATE" => {
            let partners = index.duplicates.get(&key).map(|g| {
                g.entries
                    .iter()
                    .filter(|e| !(e.file == entry.file && e.line == entry.line))
                    .map(|e| format!("{}:{}", e.file, e.line))
                    .collect()
            });
            format_partners(&entry.detail, partners.unwrap_or_default(), "with")
        }
        "FRAGMENT" => {
            let partners = index.fragments.get(&key).map(|g| {
                g.entries
                    .iter()
                    .filter(|e| !(e.file == entry.file && e.start_line == entry.line))
                    .map(|e| format!("{}:{}", e.file, e.start_line))
                    .collect()
            });
            format_partners(&entry.detail, partners.unwrap_or_default(), "also in")
        }
        "COGNITIVE" => with_max(config.complexity.max_cognitive),
        "CYCLOMATIC" => with_max(config.complexity.max_cyclomatic),
        "LONG_FN" => with_max(config.complexity.max_function_lines),
        "NESTING" => with_max(config.complexity.max_nesting_depth),
        "SRP_STRUCT" => enrich_srp_struct(entry, index.srp_structs.get(&key).copied()),
        "SRP_MODULE" => enrich_srp_module(
            entry,
            index.srp_modules.get(entry.file.as_str()).copied(),
            config,
        ),
        "SRP_PARAMS" => with_max(config.srp.max_parameters),
        "SDP" => enrich_sdp(entry, index.sdp.get(entry.detail.as_str()).copied()),
        "BOILERPLATE" => enrich_boilerplate(entry, index.boilerplate.get(&key).copied()),
        "DEAD_CODE" => enrich_dead_code(entry, index.dead_code.get(&key).copied()),
        "STRUCTURAL" => enrich_structural(entry, index.structural.get(&key).copied()),
        "ORPHAN_SUPPRESSION" => {
            enrich_orphan_suppression(entry, index.orphan_suppressions.get(&key).copied())
        }
        _ => entry.detail.clone(),
    }
}

/// Enrich orphan-suppression detail with the original marker's reason
/// (if any), so the AI agent knows what intent the stale marker had.
/// Operation: format logic, no own calls.
fn enrich_orphan_suppression(
    entry: &crate::report::findings_list::FindingEntry,
    warning: Option<&crate::adapters::report::OrphanSuppressionWarning>,
) -> String {
    let Some(w) = warning else {
        return entry.detail.clone();
    };
    match &w.reason {
        Some(r) => format!("{} — {}", entry.detail, r),
        None => entry.detail.clone(),
    }
}

/// Enrich SRP module detail with both length and cluster drivers so the
/// AI sees whichever threshold the finding is actually triggered by (both
/// if both are active). Falls back to the old "N (max M)" shape when no
/// driver flag is set.
/// Operation: format logic, no own calls.
fn enrich_srp_module(
    entry: &crate::report::findings_list::FindingEntry,
    warning: Option<&crate::adapters::analyzers::srp::ModuleSrpWarning>,
    config: &crate::config::Config,
) -> String {
    let Some(w) = warning else {
        return format!("{} (max {})", entry.detail, config.srp.file_length_baseline);
    };
    let length_driver = w.length_score > 0.0;
    let cluster_driver = w.independent_clusters > config.srp.max_independent_clusters;
    let mut parts: Vec<String> = Vec::new();
    if length_driver {
        parts.push(format!(
            "{} lines (max {})",
            w.production_lines, config.srp.file_length_baseline
        ));
    }
    if cluster_driver {
        parts.push(format!(
            "{} independent clusters (max {})",
            w.independent_clusters, config.srp.max_independent_clusters
        ));
    }
    if parts.is_empty() {
        format!("{} (max {})", entry.detail, config.srp.file_length_baseline)
    } else {
        parts.join(", ")
    }
}

/// Enrich SDP detail with the concrete instability values so the
/// stability gap driving the violation is visible without a JSON round-trip.
/// Operation: format logic, no own calls.
fn enrich_sdp(
    entry: &crate::report::findings_list::FindingEntry,
    violation: Option<&crate::adapters::analyzers::coupling::sdp::SdpViolation>,
) -> String {
    let Some(v) = violation else {
        return entry.detail.clone();
    };
    format!(
        "{} -> {} (stable I={:.2} imports unstable I={:.2})",
        v.from_module, v.to_module, v.from_instability, v.to_instability
    )
}

/// Enrich boilerplate detail with description and concrete suggestion.
/// Operation: format logic, no own calls.
fn enrich_boilerplate(
    entry: &crate::report::findings_list::FindingEntry,
    find: Option<&crate::adapters::analyzers::dry::boilerplate::BoilerplateFind>,
) -> String {
    let Some(b) = find else {
        return entry.detail.clone();
    };
    format!("{}: {} — {}", b.pattern_id, b.description, b.suggestion)
}

/// Enrich dead-code detail with the actionable suggestion string.
/// Operation: format logic, no own calls.
fn enrich_dead_code(
    entry: &crate::report::findings_list::FindingEntry,
    warning: Option<&crate::adapters::analyzers::dry::dead_code::DeadCodeWarning>,
) -> String {
    let Some(w) = warning else {
        return entry.detail.clone();
    };
    format!("{} ({})", entry.detail, w.suggestion)
}

/// Enrich structural detail with the kind's human-readable message,
/// not just the two/three-letter rule code.
/// Operation: format logic, no own calls.
fn enrich_structural(
    entry: &crate::report::findings_list::FindingEntry,
    warning: Option<&crate::adapters::analyzers::structural::StructuralWarning>,
) -> String {
    let Some(w) = warning else {
        return entry.detail.clone();
    };
    format!("{}: {}", w.kind.code(), w.kind.detail())
}

/// Enrich SRP struct detail with method and field counts.
/// Operation: format logic, no own calls.
fn enrich_srp_struct(
    entry: &crate::report::findings_list::FindingEntry,
    warning: Option<&crate::adapters::analyzers::srp::SrpWarning>,
) -> String {
    let Some(w) = warning else {
        return entry.detail.clone();
    };
    format!(
        "{}, {} methods, {} fields",
        entry.detail, w.method_count, w.field_count
    )
}

/// Enrich violation detail with logic and call line numbers.
/// Operation: format logic, no own calls.
fn enrich_violation(
    entry: &crate::report::findings_list::FindingEntry,
    fa: Option<&crate::adapters::analyzers::iosp::FunctionAnalysis>,
) -> String {
    let Some(fa) = fa else {
        return entry.detail.clone();
    };
    if let crate::adapters::analyzers::iosp::Classification::Violation {
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

/// Format partner locations into enriched detail.
/// Operation: format logic, no own calls.
fn format_partners(detail: &str, partners: Vec<String>, join_word: &str) -> String {
    if partners.is_empty() {
        return detail.to_string();
    }
    format!("{detail} {join_word} {}", partners.join(", "))
}

/// Key used for findings without a file location (e.g., coupling, cycles, SDP).
pub(crate) const GLOBAL_FILE_KEY: &str = "<global>";

/// Map FindingEntry.category to human-readable snake_case for AI output.
/// Operation: match expression, no own calls.
pub(crate) fn map_category(cat: &str) -> &str {
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
        "ORPHAN_SUPPRESSION" => "orphan_suppression",
        other => other,
    }
}
