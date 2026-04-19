use crate::report::sarif::rules::*;

#[test]
fn test_sarif_rules_contain_boilerplate_patterns() {
    let rules = sarif_rules();
    let ids: Vec<&str> = rules.iter().filter_map(|r| r["id"].as_str()).collect();
    for bp in [
        "BP-001", "BP-002", "BP-003", "BP-004", "BP-005", "BP-006", "BP-007", "BP-008", "BP-009",
        "BP-010",
    ] {
        assert!(ids.contains(&bp), "SARIF rules should contain {bp}");
    }
}
