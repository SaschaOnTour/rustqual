//! Tests for `Finding` message rendering.
//!
//! Round 16 P3 (Codex): the call-parity Check-A diagnostic used to
//! emit "within {call_depth} hops", but `call_depth` was renamed to
//! call-edge depth (round 11). The legacy "hops" wording kept the
//! off-by-one ambiguity alive — `3` means three call edges and two
//! intermediate helpers, not three nodes. Lock the new wording.

use crate::adapters::analyzers::architecture::rendering::format_match_message;
use crate::adapters::analyzers::architecture::ViolationKind;

fn missing_adapter(target: &str, missing: Vec<&str>, hint: Option<&str>) -> ViolationKind {
    ViolationKind::CallParityMissingAdapter {
        target_fn: target.into(),
        target_layer: "application".into(),
        reached_adapters: vec!["cli".into()],
        missing_adapters: missing.into_iter().map(String::from).collect(),
        hint: hint.map(String::from),
    }
}

#[test]
fn no_delegation_message_uses_call_edge_wording_not_hops() {
    let kind = ViolationKind::CallParityNoDelegation {
        fn_name: "cmd_sync".into(),
        adapter_layer: "cli".into(),
        target_layer: "application".into(),
        call_depth: 3,
    };

    let msg = format_match_message(&kind, "call parity");

    assert!(
        msg.contains('3'),
        "message must surface the configured call_depth value; got {msg:?}",
    );
    assert!(
        !msg.contains("hops"),
        "message must not use the ambiguous 'hops' wording any longer; got {msg:?}",
    );
    assert!(
        msg.contains("call edges"),
        "message must describe call_depth as call edges, matching the \
         config doc + book wording; got {msg:?}",
    );
}

#[test]
fn missing_adapter_message_embeds_hint_when_present() {
    // The `hint` field on CallParityMissingAdapter must surface in
    // the rendered Finding.message string — every output sink (text,
    // JSON, SARIF, GitHub annotation, AI rows, findings_list)
    // ultimately reads `Finding.message`, so embedding the hint here
    // covers all of them in one shot.
    let kind = missing_adapter(
        "crate::application::session::RlmSession::open",
        vec!["mcp"],
        Some(
            "1 private method in mcp ...\n  - src/mcp/server.rs:67 search has #[tool] attribute(s)",
        ),
    );

    let msg = format_match_message(&kind, "call parity");

    assert!(
        msg.contains("is not reached from adapter layer(s): mcp"),
        "head text must remain intact when a hint is appended; got {msg:?}"
    );
    assert!(
        msg.contains("hint:"),
        "rendered message must announce the hint section so renderers \
         can identify it; got {msg:?}"
    );
    assert!(
        msg.contains("#[tool]") && msg.contains("src/mcp/server.rs:67"),
        "rendered message must surface the hint payload verbatim; got {msg:?}"
    );
}

#[test]
fn missing_adapter_message_omits_hint_section_when_none() {
    // No hint → no `hint:` section. Avoids dangling labels and keeps
    // the legacy single-line format for findings without promotion
    // candidates.
    let kind = missing_adapter(
        "crate::application::session::RlmSession::open",
        vec!["mcp"],
        None,
    );

    let msg = format_match_message(&kind, "call parity");

    assert!(
        !msg.contains("hint:"),
        "no hint must mean no hint section; got {msg:?}"
    );
}

#[test]
fn multiplicity_mismatch_head_lists_per_adapter_counts() {
    let kind = ViolationKind::CallParityMultiplicityMismatch {
        target_fn: "handle".into(),
        target_layer: "domain".into(),
        counts_per_adapter: vec![("cli".into(), 1), ("http".into(), 2)],
    };
    let msg = format_match_message(&kind, "call parity");
    assert!(msg.contains("divergent handler counts"), "{msg}");
    assert!(msg.contains("cli=1") && msg.contains("http=2"), "{msg}");
}

#[test]
fn multi_touchpoint_head_lists_touchpoints() {
    let kind = ViolationKind::CallParityMultiTouchpoint {
        fn_name: "dispatch".into(),
        adapter_layer: "adapters".into(),
        touchpoints: vec!["a".into(), "b".into()],
    };
    let msg = format_match_message(&kind, "call parity");
    assert!(msg.contains("multiple target touchpoints"), "{msg}");
    assert!(msg.contains("adapters::dispatch"), "{msg}");
    assert!(msg.contains('a') && msg.contains('b'), "{msg}");
}

#[test]
fn item_kind_head_omits_empty_name() {
    let named = format_match_message(
        &ViolationKind::ItemKind {
            kind: "struct",
            name: "Foo".into(),
        },
        "r",
    );
    assert!(named.contains("struct Foo"), "named: {named}");
    let anon = format_match_message(
        &ViolationKind::ItemKind {
            kind: "struct",
            name: String::new(),
        },
        "r",
    );
    assert!(anon.contains("struct"), "anon kind: {anon}");
    assert!(
        !anon.contains("struct "),
        "anonymous item omits the name: {anon}"
    );
}

fn item_loc() -> crate::adapters::analyzers::architecture::MatchLocation {
    crate::adapters::analyzers::architecture::MatchLocation {
        file: "src/x.rs".into(),
        line: 9,
        column: 4,
        kind: ViolationKind::ItemKind {
            kind: "fn",
            name: "bad".into(),
        },
    }
}

#[test]
fn match_to_finding_projects_every_field() {
    use crate::adapters::analyzers::architecture::rendering::match_to_finding;
    let pattern = crate::adapters::config::architecture::SymbolPattern {
        name: "p".into(),
        allowed_in: None,
        forbidden_in: None,
        except: vec![],
        forbid_path_prefix: None,
        forbid_method_call: None,
        forbid_function_call: None,
        forbid_macro_call: None,
        forbid_item_kind: None,
        forbid_derive: None,
        forbid_glob_import: None,
        regex: None,
        reason: "no anon fns".into(),
    };
    let f = match_to_finding(item_loc(), "architecture/pattern/x", &pattern);
    assert_eq!(f.file, "src/x.rs");
    assert_eq!(f.line, 9);
    assert_eq!(f.column, 4);
    assert_eq!(f.dimension, crate::domain::Dimension::Architecture);
    assert_eq!(f.rule_id, "architecture/pattern/x");
    assert_eq!(f.severity, crate::domain::Severity::Medium);
    assert!(
        f.message.contains("no anon fns"),
        "reason in message: {}",
        f.message
    );
}

#[test]
fn build_architecture_finding_projects_every_field() {
    use crate::adapters::analyzers::architecture::rendering::build_architecture_finding;
    let f = build_architecture_finding(
        item_loc(),
        "architecture/layer".into(),
        "reason text",
        crate::domain::Severity::High,
    );
    assert_eq!(f.file, "src/x.rs");
    assert_eq!(f.line, 9);
    assert_eq!(f.column, 4);
    assert_eq!(f.dimension, crate::domain::Dimension::Architecture);
    assert_eq!(f.rule_id, "architecture/layer");
    assert_eq!(f.severity, crate::domain::Severity::High);
}

#[test]
fn build_file_refs_maps_each_parsed_file() {
    use crate::adapters::analyzers::architecture::rendering::build_file_refs;
    use crate::ports::{AnalysisContext, ParsedFile};
    let config = crate::config::Config::default();
    let files = vec![ParsedFile {
        path: "src/a.rs".into(),
        content: "fn f() {}".into(),
        ast: syn::parse_file("fn f() {}").unwrap(),
    }];
    let ctx = AnalysisContext {
        files: &files,
        config: &config,
    };
    let refs = build_file_refs(&ctx);
    assert_eq!(refs.len(), 1, "one ref per parsed file");
    assert_eq!(refs[0].0, "src/a.rs");
}
