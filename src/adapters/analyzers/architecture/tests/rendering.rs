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
