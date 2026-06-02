use super::*;

#[test]
fn test_print_json_empty_yields_valid_json() {
    let analysis = make_analysis(vec![]);
    let v = json_value(&analysis);
    assert_eq!(
        v["functions"].as_array().map(Vec::len),
        Some(0),
        "empty analysis produces empty functions array"
    );
}

#[test]
fn test_print_json_carries_violation_logic_and_call_locations() {
    let analysis = make_analysis(vec![make_result(
        "bad_fn",
        Classification::Violation {
            has_logic: true,
            has_own_calls: true,
            logic_locations: vec![LogicOccurrence {
                kind: "if".into(),
                line: 1,
            }],
            call_locations: vec![CallOccurrence {
                name: "f".into(),
                line: 2,
            }],
        },
    )]);
    let v = json_value(&analysis);
    let f = function_named(&v, "bad_fn");
    assert_eq!(f["classification"], "violation");
    let logic = f["logic"].as_array().expect("logic array");
    assert_eq!(logic.len(), 1, "one logic location; got {v}");
    assert_eq!(logic[0]["kind"], "if");
    assert_eq!(logic[0]["line"], "1");
    let calls = f["calls"].as_array().expect("calls array");
    assert_eq!(calls.len(), 1, "one call location; got {v}");
    assert_eq!(calls[0]["name"], "f");
    assert_eq!(calls[0]["line"], "2");
}

#[test]
fn test_print_json_classifications_all_four() {
    let analysis = make_analysis(vec![
        make_result("a", Classification::Integration),
        make_result("b", Classification::Operation),
        make_result("c", Classification::Trivial),
        make_result(
            "d",
            Classification::Violation {
                has_logic: true,
                has_own_calls: true,
                logic_locations: vec![LogicOccurrence {
                    kind: "match".into(),
                    line: 1,
                }],
                call_locations: vec![CallOccurrence {
                    name: "g".into(),
                    line: 2,
                }],
            },
        ),
    ]);
    let v = json_value(&analysis);
    let by_name: std::collections::HashMap<String, String> = v["functions"]
        .as_array()
        .expect("functions array")
        .iter()
        .map(|f| {
            (
                f["name"].as_str().unwrap().into(),
                f["classification"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(by_name.get("a").map(String::as_str), Some("integration"));
    assert_eq!(by_name.get("b").map(String::as_str), Some("operation"));
    assert_eq!(by_name.get("c").map(String::as_str), Some("trivial"));
    assert_eq!(by_name.get("d").map(String::as_str), Some("violation"));
}

#[test]
fn test_print_json_carries_near_duplicate_similarity() {
    use crate::domain::findings::{
        DryFinding, DryFindingDetails, DryFindingKind, DuplicateParticipant,
    };
    use crate::domain::{Dimension, Finding, Severity};
    let participants = vec![
        DuplicateParticipant {
            function_name: "a".into(),
            file: "lib.rs".into(),
            line: 10,
        },
        DuplicateParticipant {
            function_name: "b".into(),
            file: "lib.rs".into(),
            line: 50,
        },
    ];
    let mut analysis = make_analysis(vec![]);
    analysis.findings.dry.push(DryFinding {
        common: Finding {
            file: "lib.rs".into(),
            line: 10,
            column: 0,
            dimension: Dimension::Dry,
            rule_id: "dry/duplicate/similar".into(),
            message: "near duplicate".into(),
            severity: Severity::Medium,
            suppressed: false,
        },
        kind: DryFindingKind::DuplicateSimilar,
        details: DryFindingDetails::Duplicate {
            participants,
            similarity: Some(0.91),
        },
    });
    let v = json_value(&analysis);
    let group = v["duplicates"]
        .as_array()
        .and_then(|a| a.first())
        .expect("at least one duplicate group");
    let sim = group["similarity"].as_f64();
    assert_eq!(
        sim,
        Some(0.91),
        "NearDuplicate similarity must survive projection + reporter; got {v}"
    );
}

#[test]
fn test_print_json_carries_repeated_match_arm_count_and_distinct_groups() {
    use crate::domain::findings::{
        DryFinding, DryFindingDetails, DryFindingKind, RepeatedMatchParticipant,
    };
    use crate::domain::{Dimension, Finding, Severity};
    let common = |line: usize| Finding {
        file: "lib.rs".into(),
        line,
        column: 0,
        dimension: Dimension::Dry,
        rule_id: "dry/repeated_match".into(),
        message: "repeated match".into(),
        severity: Severity::Medium,
        suppressed: false,
    };
    let participant = |name: &str, line: usize, arms: usize| RepeatedMatchParticipant {
        function_name: name.into(),
        file: "lib.rs".into(),
        line,
        arm_count: arms,
    };
    let group_a = vec![participant("fa1", 10, 4), participant("fa2", 20, 4)];
    let group_b = vec![participant("fb1", 50, 3), participant("fb2", 60, 3)];
    let make_match = |line: usize, participants: Vec<RepeatedMatchParticipant>| DryFinding {
        common: common(line),
        kind: DryFindingKind::RepeatedMatch,
        details: DryFindingDetails::RepeatedMatch {
            enum_name: "MyEnum".into(),
            participants,
        },
    };
    let mut analysis = make_analysis(vec![]);
    analysis.findings.dry.push(make_match(10, group_a));
    analysis.findings.dry.push(make_match(50, group_b));
    let v = json_value(&analysis);
    let groups = v["repeated_matches"]
        .as_array()
        .expect("repeated_matches array");
    assert_eq!(
        groups.len(),
        2,
        "two distinct groups over same enum must NOT collapse by enum_name; got {v}"
    );
    let arm_counts: Vec<u64> = groups
        .iter()
        .flat_map(|g| g["entries"].as_array().unwrap().iter())
        .map(|e| e["arm_count"].as_u64().unwrap_or(0))
        .collect();
    assert!(
        arm_counts.contains(&4) && arm_counts.contains(&3),
        "arm_count must survive projection (expected both 4 and 3); got {arm_counts:?} from {v}"
    );
}
