use crate::domain::Dimension;

#[test]
fn parse_known_dimensions_case_insensitive() {
    assert_eq!(Dimension::from_str_opt("iosp"), Some(Dimension::Iosp));
    assert_eq!(Dimension::from_str_opt("IOSP"), Some(Dimension::Iosp));
    assert_eq!(Dimension::from_str_opt("Iosp"), Some(Dimension::Iosp));
    assert_eq!(
        Dimension::from_str_opt("complexity"),
        Some(Dimension::Complexity)
    );
    assert_eq!(Dimension::from_str_opt("dry"), Some(Dimension::Dry));
    assert_eq!(Dimension::from_str_opt("srp"), Some(Dimension::Srp));
    assert_eq!(
        Dimension::from_str_opt("coupling"),
        Some(Dimension::Coupling)
    );
}

#[test]
fn parse_test_quality_accepts_all_aliases() {
    // The Test-Quality dimension is reachable by three alias spellings:
    // - "test_quality" (canonical, matches the [weights] field)
    // - "tq" (short form used in internal docs)
    // - "test" (legacy spelling, kept for backward compatibility when
    //   reading suppression comments written against older rustqual versions)
    assert_eq!(
        Dimension::from_str_opt("test_quality"),
        Some(Dimension::TestQuality)
    );
    assert_eq!(Dimension::from_str_opt("tq"), Some(Dimension::TestQuality));
    assert_eq!(
        Dimension::from_str_opt("test"),
        Some(Dimension::TestQuality)
    );
}

#[test]
fn parse_architecture_dimension() {
    // New in v1.0: Architecture is the seventh dimension.
    assert_eq!(
        Dimension::from_str_opt("architecture"),
        Some(Dimension::Architecture)
    );
    assert_eq!(
        Dimension::from_str_opt("Architecture"),
        Some(Dimension::Architecture)
    );
}

#[test]
fn parse_unknown_returns_none() {
    assert_eq!(Dimension::from_str_opt(""), None);
    assert_eq!(Dimension::from_str_opt("unknown"), None);
    assert_eq!(Dimension::from_str_opt("performance"), None);
}

#[test]
fn display_matches_canonical_name() {
    assert_eq!(format!("{}", Dimension::Iosp), "iosp");
    assert_eq!(format!("{}", Dimension::Complexity), "complexity");
    assert_eq!(format!("{}", Dimension::Dry), "dry");
    assert_eq!(format!("{}", Dimension::Srp), "srp");
    assert_eq!(format!("{}", Dimension::Coupling), "coupling");
    assert_eq!(format!("{}", Dimension::TestQuality), "test_quality");
    assert_eq!(format!("{}", Dimension::Architecture), "architecture");
}

#[test]
fn display_roundtrips_via_from_str_opt() {
    let all = [
        Dimension::Iosp,
        Dimension::Complexity,
        Dimension::Dry,
        Dimension::Srp,
        Dimension::Coupling,
        Dimension::TestQuality,
        Dimension::Architecture,
    ];
    for d in all {
        let s = format!("{d}");
        assert_eq!(
            Dimension::from_str_opt(&s),
            Some(d),
            "Display output must roundtrip through from_str_opt for {d:?}"
        );
    }
}
