use crate::domain::{target_kind, target_names, Dimension, Suppression, SuppressionTarget};

fn metric(dim: Dimension, name: &str, pin: f64) -> Suppression {
    Suppression {
        line: 1,
        dimensions: vec![dim],
        reason: Some("r".into()),
        target: Some(SuppressionTarget::Metric {
            name: name.into(),
            pin,
        }),
    }
}

#[test]
fn metric_pin_suppresses_at_or_below_and_refires_above() {
    let s = metric(Dimension::Srp, "file_length", 400.0);
    assert!(
        s.suppresses(Dimension::Srp, "file_length", Some(400.0)),
        "at the pin"
    );
    assert!(
        s.suppresses(Dimension::Srp, "file_length", Some(399.0)),
        "below the pin"
    );
    assert!(
        !s.suppresses(Dimension::Srp, "file_length", Some(401.0)),
        "above → re-fires"
    );
}

#[test]
fn metric_pin_never_silences_a_value_less_finding() {
    // The whole point of the redesign: a Metric target queried with no value
    // fails closed (does not suppress). A `map_or(true, …)` regression here
    // would silently over-suppress.
    let s = metric(Dimension::Srp, "file_length", 400.0);
    assert!(!s.suppresses(Dimension::Srp, "file_length", None));
}

#[test]
fn metric_pin_only_matches_its_own_target_and_dimension() {
    let s = metric(Dimension::Srp, "file_length", 400.0);
    assert!(
        !s.suppresses(Dimension::Srp, "max_parameters", Some(1.0)),
        "wrong target"
    );
    assert!(
        !s.suppresses(Dimension::Complexity, "file_length", Some(1.0)),
        "wrong dim"
    );
}

#[test]
fn boolean_target_silences_by_name_regardless_of_value() {
    let s = Suppression {
        line: 1,
        dimensions: vec![Dimension::Srp],
        reason: Some("r".into()),
        target: Some(SuppressionTarget::Boolean {
            name: "god_struct".into(),
        }),
    };
    assert!(s.suppresses(Dimension::Srp, "god_struct", None));
    assert!(s.suppresses(Dimension::Srp, "god_struct", Some(9.0)));
    assert!(
        !s.suppresses(Dimension::Srp, "file_length", Some(1.0)),
        "wrong target"
    );
}

#[test]
fn blanket_suppression_silences_every_finding_of_its_dimension() {
    let s = Suppression {
        line: 1,
        dimensions: vec![Dimension::Dry],
        reason: None,
        target: None,
    };
    assert!(s.suppresses(Dimension::Dry, "duplicate", None));
    assert!(s.suppresses(Dimension::Dry, "anything", Some(5.0)));
    assert!(
        !s.suppresses(Dimension::Srp, "duplicate", None),
        "wrong dim"
    );
}

#[test]
fn target_kind_and_target_names_agree_for_every_dimension() {
    // The two hand-maintained vocabularies must not drift: every name in
    // `target_names(dim)` must resolve via `target_kind(dim, name)`, else a
    // valid-looking target would be rejected by the parser or unmatchable by
    // the orphan detector.
    for dim in [
        Dimension::Iosp,
        Dimension::Complexity,
        Dimension::Dry,
        Dimension::Srp,
        Dimension::Coupling,
        Dimension::TestQuality,
        Dimension::Architecture,
    ] {
        for name in target_names(dim) {
            assert!(
                target_kind(dim, name).is_some(),
                "{dim:?} lists target {name:?} but target_kind doesn't classify it"
            );
        }
        assert!(target_kind(dim, "definitely_not_a_target").is_none());
    }
}

#[test]
fn empty_dimensions_list_covers_everything() {
    let s = Suppression {
        line: 1,
        dimensions: vec![],
        reason: None,
        target: None,
    };
    assert!(s.covers(Dimension::Iosp));
    assert!(s.covers(Dimension::Complexity));
    assert!(s.covers(Dimension::Architecture));
}

#[test]
fn specific_dimensions_only_cover_those_listed() {
    let s = Suppression {
        line: 1,
        dimensions: vec![Dimension::Iosp],
        reason: None,
        target: None,
    };
    assert!(s.covers(Dimension::Iosp));
    assert!(!s.covers(Dimension::Complexity));
    assert!(!s.covers(Dimension::Architecture));
}

#[test]
fn multiple_dimensions_cover_all_listed() {
    let s = Suppression {
        line: 1,
        dimensions: vec![Dimension::Iosp, Dimension::Architecture],
        reason: Some("migration in progress".into()),
        target: None,
    };
    assert!(s.covers(Dimension::Iosp));
    assert!(s.covers(Dimension::Architecture));
    assert!(!s.covers(Dimension::Dry));
}
