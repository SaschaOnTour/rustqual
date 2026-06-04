//! End-to-end `analyze_coupling` tests: real `use` statements →
//! graph → metrics → cycles → SDP. The unit-level mechanics live in
//! `root.rs` (graph/metrics/cycles) and `sdp.rs` (the SDP algorithm).

use super::make_parsed;
use crate::adapters::analyzers::coupling::*;

#[test]
fn test_analyze_coupling_integration() {
    let parsed = make_parsed(vec![
        ("main.rs", "use crate::config::Config; fn main() {}"),
        ("config.rs", "pub struct Config;"),
        ("pipeline.rs", "use crate::config::Config; pub fn run() {}"),
    ]);
    let analysis = analyze_coupling(&parsed);
    assert_eq!(analysis.metrics.len(), 3);
    assert!(analysis.cycles.is_empty());

    // config should have highest afferent coupling (2 dependents)
    let config_metrics = analysis
        .metrics
        .iter()
        .find(|m| m.module_name == "config")
        .unwrap();
    assert_eq!(config_metrics.afferent, 2);
    assert_eq!(config_metrics.efferent, 0);
}

#[test]
fn test_analyze_coupling_with_cycle() {
    let parsed = make_parsed(vec![
        ("a.rs", "use crate::b::Foo; pub struct Bar;"),
        ("b.rs", "use crate::a::Bar; pub struct Foo;"),
    ]);
    let analysis = analyze_coupling(&parsed);
    assert_eq!(analysis.cycles.len(), 1);
    assert!(analysis.cycles[0].modules.contains(&"a".to_string()));
    assert!(analysis.cycles[0].modules.contains(&"b".to_string()));
}

#[test]
fn test_analyze_coupling_no_crate_deps() {
    let parsed = make_parsed(vec![
        ("a.rs", "use std::collections::HashMap; fn f() {}"),
        ("b.rs", "use serde::Deserialize; fn g() {}"),
    ]);
    let analysis = analyze_coupling(&parsed);
    assert!(analysis.cycles.is_empty());
    for m in &analysis.metrics {
        assert_eq!(m.afferent, 0);
        assert_eq!(m.efferent, 0);
    }
}

#[test]
fn analyze_coupling_surfaces_sdp_violation_from_real_source() {
    // End-to-end CP-002: real `use` statements build a graph where stable `a`
    // (depended on by x,y; depends only on b) imports the unstable `b` (depends
    // on p,q). The SDP check (via `populate_sdp_violations`, the pipeline's
    // path) must surface the a→b inversion. The other coupling tests drive the
    // SDP algorithm with hand-built `ModuleGraph`s; this pins the full
    // source → graph → metrics → SDP composition.
    let parsed = make_parsed(vec![
        ("a.rs", "use crate::b::B; pub struct A;"),
        ("b.rs", "use crate::p::P; use crate::q::Q; pub struct B;"),
        ("x.rs", "use crate::a::A; pub struct X;"),
        ("y.rs", "use crate::a::A; pub struct Y;"),
        ("p.rs", "pub struct P;"),
        ("q.rs", "pub struct Q;"),
    ]);
    let mut analysis = analyze_coupling(&parsed);
    assert!(
        analysis.sdp_violations.is_empty(),
        "sdp_violations is empty until populate_sdp_violations runs"
    );
    populate_sdp_violations(&mut analysis);
    let edge = analysis
        .sdp_violations
        .iter()
        .find(|v| v.from_module == "a" && v.to_module == "b");
    let v = edge.expect("a→b SDP violation surfaced from source");
    assert!(
        v.from_instability < v.to_instability,
        "stable a (I={}) depends on less stable b (I={})",
        v.from_instability,
        v.to_instability
    );
}
