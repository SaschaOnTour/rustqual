use super::*;

#[test]
fn lcom4_scenarios() {
    let cases: &[Lcom4Case] = &[
        (
            "all methods share one field → fully cohesive",
            &[("a", &["x"], &[]), ("b", &["x"], &[]), ("c", &["x"], &[])],
            &["x", "y"],
            1,
            Some(1),
        ),
        (
            "two disjoint method groups → two clusters",
            &[
                ("a", &["x"], &[]),
                ("b", &["x"], &[]),
                ("c", &["y"], &[]),
                ("d", &["y"], &[]),
            ],
            &["x", "y"],
            2,
            Some(2),
        ),
        (
            "each method a unique field → N components",
            &[("a", &["x"], &[]), ("b", &["y"], &[]), ("c", &["z"], &[])],
            &["x", "y", "z"],
            3,
            None,
        ),
        ("no methods → LCOM4 0, no clusters", &[], &["x"], 0, Some(0)),
        (
            "method with no field access → isolated component",
            &[("a", &["x"], &[]), ("b", &[], &["helper"])],
            &["x"],
            2,
            None,
        ),
    ];
    for (label, method_specs, field_names, exp_lcom4, exp_clusters) in cases {
        let methods: Vec<MethodFieldData> = method_specs
            .iter()
            .map(|(n, f, c)| make_method(n, "Foo", f, c))
            .collect();
        let refs: Vec<&MethodFieldData> = methods.iter().collect();
        let fields: Vec<String> = field_names.iter().map(|s| s.to_string()).collect();
        let (lcom4, clusters) =
            compute_lcom4(&refs, &fields, &build_field_method_index(&refs, &fields));
        assert_eq!(lcom4, *exp_lcom4, "case: {label}");
        if let Some(c) = exp_clusters {
            assert_eq!(clusters.len(), *c, "case: {label}");
        }
    }
}

#[test]
fn test_fan_out_distinct_targets() {
    let m1 = make_method("a", "Foo", &[], &["helper", "process"]);
    let m2 = make_method("b", "Foo", &[], &["helper", "format"]);
    let methods: Vec<&MethodFieldData> = vec![&m1, &m2];
    let fan_out = compute_fan_out(&methods);
    assert_eq!(fan_out, 3); // helper, process, format
}

#[test]
fn test_fan_out_empty() {
    let m1 = make_method("a", "Foo", &["x"], &[]);
    let methods: Vec<&MethodFieldData> = vec![&m1];
    let fan_out = compute_fan_out(&methods);
    assert_eq!(fan_out, 0);
}

#[test]
fn test_composite_score_fully_cohesive() {
    let config = SrpConfig::default();
    // LCOM4=1, small struct → low score
    let score = compute_composite_score(1, 3, 3, 0, &config);
    assert!(
        score < config.smell_threshold,
        "Cohesive struct should score below threshold, got {score}"
    );
}

#[test]
fn test_composite_score_high_lcom4() {
    let config = SrpConfig::default();
    // LCOM4=4, many fields, many methods, high fan-out
    let score = compute_composite_score(4, 15, 20, 12, &config);
    assert!(
        score >= config.smell_threshold,
        "Incohesive struct should exceed threshold, got {score}"
    );
}

#[test]
fn test_composite_score_lcom4_one_is_zero() {
    let config = SrpConfig::default();
    let score_cohesive = compute_composite_score(1, 5, 5, 2, &config);
    let score_incohesive = compute_composite_score(3, 5, 5, 2, &config);
    assert!(score_incohesive > score_cohesive);
}
