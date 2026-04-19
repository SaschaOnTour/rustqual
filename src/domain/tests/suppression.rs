use crate::domain::{Dimension, Suppression};

#[test]
fn empty_dimensions_list_covers_everything() {
    let s = Suppression {
        line: 1,
        dimensions: vec![],
        reason: None,
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
    };
    assert!(s.covers(Dimension::Iosp));
    assert!(s.covers(Dimension::Architecture));
    assert!(!s.covers(Dimension::Dry));
}
