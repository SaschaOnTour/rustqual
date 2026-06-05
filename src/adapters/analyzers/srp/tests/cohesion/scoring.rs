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
        let (lcom4, clusters) = compute_lcom4(
            &refs,
            &fields,
            &build_field_method_index(&refs, &fields),
            &[],
        );
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

#[test]
fn test_composite_score_exact_value() {
    // Pin the whole formula to one exact number so every arithmetic step is
    // observable. With threshold-range 4 and distinct weights:
    //   lcom4_norm  = (3-1)/4            = 0.5  → 0.4 * 0.5  = 0.20
    //   field_norm  = 1/4                = 0.25 → 0.3 * 0.25 = 0.075
    //   method_norm = 5/10               = 0.5  → 0.2 * 0.5  = 0.10
    //   fan_out_norm= 1/5                = 0.2  → 0.1 * 0.2  = 0.02
    //   composite                                            = 0.395
    let config = SrpConfig {
        lcom4_threshold: 5,
        max_fields: 4,
        max_methods: 10,
        max_fan_out: 5,
        weights: [0.4, 0.3, 0.2, 0.1],
        ..SrpConfig::default()
    };
    let score = compute_composite_score(3, 1, 5, 1, &config);
    assert!(
        (score - 0.395).abs() < 1e-9,
        "expected composite 0.395, got {score}"
    );
}

#[test]
fn test_composite_score_zero_cap_ratio() {
    // A misconfigured `max_fields == 0` must treat zero count as 0.0 and any
    // non-zero count as fully penalised (1.0) — guards `normalised_ratio`'s
    // `value == 0` branch. Weight only the field term so it drives the score.
    let config = SrpConfig {
        max_fields: 0,
        weights: [0.0, 1.0, 0.0, 0.0],
        ..SrpConfig::default()
    };
    assert_eq!(
        compute_composite_score(0, 0, 0, 0, &config),
        0.0,
        "zero count under a zero cap is unpenalised"
    );
    assert_eq!(
        compute_composite_score(0, 2, 0, 0, &config),
        1.0,
        "non-zero count under a zero cap is fully penalised"
    );
}
