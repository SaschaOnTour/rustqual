//! Tests for the rule-card registry — the single source of truth every
//! reporter (SARIF, `--explain`, summary titles) renders rule metadata from.

use crate::domain::rule_cards::{all_rule_cards, find_rule_card};

#[test]
fn find_is_case_insensitive() {
    let lower = find_rule_card("bp-009").expect("bp-009 must resolve");
    let upper = find_rule_card("BP-009").expect("BP-009 must resolve");
    assert_eq!(lower.id, "BP-009");
    assert_eq!(upper.id, "BP-009");
}

#[test]
fn unknown_id_is_none() {
    assert!(find_rule_card("BP-999").is_none());
    assert!(find_rule_card("not-a-rule").is_none());
    assert!(find_rule_card("").is_none());
}

#[test]
fn ids_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for card in all_rule_cards() {
        assert!(seen.insert(card.id), "duplicate rule-card id {}", card.id);
    }
}

#[test]
fn every_card_is_fully_filled() {
    // Every field must carry substantive content — an empty `suppress` or
    // `config` would leave a consumer without a next step, which is exactly
    // the failure mode the cards exist to prevent. "Not suppressible" etc.
    // must be said explicitly.
    for card in all_rule_cards() {
        for (name, value) in [
            ("title", card.title),
            ("detects", card.detects),
            ("why", card.why),
            ("fix", card.fix),
            ("suppress", card.suppress),
            ("config", card.config),
        ] {
            assert!(
                value.len() >= 10,
                "card {} field `{name}` must be substantive, got {value:?}",
                card.id
            );
        }
    }
}

#[test]
fn every_rule_family_is_represented() {
    // One spot-check per family so a forgotten module can't slip through.
    for id in [
        "iosp/violation",
        "CX-001",
        "A20",
        "DRY-002",
        "BP-002",
        "SRP-001",
        "CP-003",
        "TQ-003",
        "SLM",
        "IET",
        "architecture/pattern",
        "architecture/call_parity/no_delegation",
        "SUP-001",
        "ORPHAN-001",
    ] {
        assert!(find_rule_card(id).is_some(), "missing rule card for {id}");
    }
}

#[test]
fn cards_name_real_suppression_targets() {
    // The suppress field must cite the actual `qual:allow` vocabulary, not an
    // invented name — an agent copies this line verbatim.
    let bp = find_rule_card("BP-002").unwrap();
    assert!(
        bp.suppress.contains("qual:allow(dry, boilerplate)"),
        "BP-002 suppress must cite the dry/boilerplate target: {}",
        bp.suppress
    );
    let cx = find_rule_card("CX-001").unwrap();
    assert!(
        cx.suppress
            .contains("qual:allow(complexity, max_cognitive="),
        "CX-001 suppress must cite the pinned metric form: {}",
        cx.suppress
    );
    let dead = find_rule_card("DRY-002").unwrap();
    assert!(
        dead.suppress.contains("qual:api"),
        "DRY-002 is excluded via qual:api, not allow(dry): {}",
        dead.suppress
    );
}

#[test]
fn cards_name_their_config_knob() {
    let bp = find_rule_card("BP-009").unwrap();
    assert!(
        bp.config.contains("[boilerplate]"),
        "BP-009 config must point at the [boilerplate] section: {}",
        bp.config
    );
    let cx = find_rule_card("CX-004").unwrap();
    assert!(
        cx.config.contains("max_function_lines"),
        "CX-004 config must name the threshold field: {}",
        cx.config
    );
}
