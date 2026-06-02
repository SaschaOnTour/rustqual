use super::*;

#[test]
fn test_param_warning_exceeds_threshold() {
    let config = SrpConfig::default();
    let results = vec![make_func("many_params", 7, false)];
    let mut srp = make_srp();
    apply_parameter_warnings(&results, Some(&mut srp), &config);
    assert_eq!(srp.param_warnings.len(), 1);
    assert_eq!(srp.param_warnings[0].parameter_count, 7);
    assert_eq!(srp.param_warnings[0].function_name, "many_params");
}

#[test]
fn test_param_warning_at_threshold_no_warning() {
    let config = SrpConfig::default();
    let results = vec![make_func("ok_params", 5, false)];
    let mut srp = make_srp();
    apply_parameter_warnings(&results, Some(&mut srp), &config);
    assert!(srp.param_warnings.is_empty(), "5 == threshold, no warning");
}

#[test]
fn test_param_warning_trait_impl_excluded() {
    let config = SrpConfig::default();
    let results = vec![make_func("trait_fn", 10, true)];
    let mut srp = make_srp();
    apply_parameter_warnings(&results, Some(&mut srp), &config);
    assert!(
        srp.param_warnings.is_empty(),
        "trait impl should be excluded"
    );
}

#[test]
fn test_param_warning_suppressed_fn_is_flagged_but_marked_suppressed() {
    // Suppressed over-threshold functions now emit a warning with
    // `suppressed=true` instead of being filtered out silently. This
    // lets the orphan-suppression checker see that a `qual:allow(srp)`
    // marker did have a target. `summary.srp_param_warnings` still
    // counts only non-suppressed entries, so user-visible behavior
    // (finding count, quality score) is unchanged.
    let config = SrpConfig::default();
    let mut func = make_func("suppressed_fn", 10, false);
    func.suppressed = true;
    let results = vec![func];
    let mut srp = make_srp();
    apply_parameter_warnings(&results, Some(&mut srp), &config);
    assert_eq!(srp.param_warnings.len(), 1, "entry recorded");
    assert!(
        srp.param_warnings[0].suppressed,
        "entry must be marked suppressed"
    );
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn test_param_warning_custom_threshold() {
    let mut config = SrpConfig::default();
    config.max_parameters = 3;
    let results = vec![make_func("four_params", 4, false)];
    let mut srp = make_srp();
    apply_parameter_warnings(&results, Some(&mut srp), &config);
    assert_eq!(srp.param_warnings.len(), 1, "4 > custom threshold 3");
}

#[test]
fn test_param_warning_srp_none() {
    let config = SrpConfig::default();
    let results = vec![make_func("fn", 10, false)];
    apply_parameter_warnings(&results, None, &config);
    // No panic, no-op when SRP is None
}
