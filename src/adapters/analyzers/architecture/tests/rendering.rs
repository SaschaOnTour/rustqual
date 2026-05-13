//! Tests for `Finding` message rendering.
//!
//! Round 16 P3 (Codex): the call-parity Check-A diagnostic used to
//! emit "within {call_depth} hops", but `call_depth` was renamed to
//! call-edge depth (round 11). The legacy "hops" wording kept the
//! off-by-one ambiguity alive — `3` means three call edges and two
//! intermediate helpers, not three nodes. Lock the new wording.

use crate::adapters::analyzers::architecture::rendering::format_match_message;
use crate::adapters::analyzers::architecture::ViolationKind;

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
