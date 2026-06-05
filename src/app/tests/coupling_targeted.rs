//! Per-kind, module-global coupling suppression: `allow(coupling, <kind>[=N])`
//! silences one finding-kind (metric pins honour their value), leaving the
//! other coupling findings of the module active. Blanket `allow(coupling)`
//! silences everything for the module.
use crate::adapters::analyzers::coupling::sdp::SdpViolation;
use crate::adapters::analyzers::coupling::{CouplingAnalysis, CouplingMetrics, ModuleGraph};
use crate::app::coupling_suppressions::{mark_sdp_target_suppressions, ModuleCouplingSuppressions};
use crate::app::metrics::count_coupling_warnings;
use crate::config::sections::CouplingConfig;
use crate::domain::SuppressionTarget;
use crate::findings::{Dimension, Suppression};
use crate::report::Summary;
use std::collections::HashMap;

/// One `foo` module that breaches instability (afferent>0, I=0.83 > 0.8).
fn analysis() -> CouplingAnalysis {
    CouplingAnalysis {
        metrics: vec![CouplingMetrics {
            module_name: "foo".to_string(),
            afferent: 1,
            efferent: 5,
            instability: 0.83,
            incoming: vec![],
            outgoing: vec![],
            suppressed: false,
            warning: false,
        }],
        cycles: vec![],
        sdp_violations: vec![],
        graph: ModuleGraph::default(),
    }
}

fn sups(target: Option<SuppressionTarget>) -> ModuleCouplingSuppressions {
    let mut sl = HashMap::new();
    sl.insert(
        "foo.rs".to_string(),
        vec![Suppression {
            line: 1,
            dimensions: vec![Dimension::Coupling],
            reason: Some("r".to_string()),
            target,
        }],
    );
    ModuleCouplingSuppressions::build(&sl)
}

fn pin(name: &str, value: Option<f64>) -> ModuleCouplingSuppressions {
    let target = match value {
        Some(pin) => SuppressionTarget::Metric {
            name: name.to_string(),
            pin,
        },
        None => SuppressionTarget::Boolean {
            name: name.to_string(),
        },
    };
    sups(Some(target))
}

fn warnings(s: &ModuleCouplingSuppressions) -> usize {
    let mut a = analysis();
    let mut summary = Summary::from_results(&[]);
    count_coupling_warnings(Some(&mut a), &CouplingConfig::default(), s, &mut summary);
    summary.coupling_warnings
}

#[test]
fn instability_pin_suppresses_within_and_refires() {
    assert_eq!(
        warnings(&pin("max_instability", Some(0.9))),
        0,
        "0.83 <= 0.9"
    );
    assert_eq!(
        warnings(&pin("max_instability", Some(0.82))),
        1,
        "0.83 > 0.82 → re-fires"
    );
}

#[test]
fn wrong_metric_target_leaves_instability_firing() {
    assert_eq!(
        warnings(&pin("max_fan_out", Some(99.0))),
        1,
        "a fan_out pin must not silence the instability warning"
    );
}

#[test]
fn blanket_allow_coupling_suppresses_module() {
    assert_eq!(warnings(&sups(None)), 0, "blanket silences the module");
}

#[test]
fn sdp_target_suppresses_sdp_not_metric() {
    let mut a = analysis();
    a.sdp_violations = vec![SdpViolation {
        from_module: "foo".to_string(),
        to_module: "bar".to_string(),
        from_instability: 0.83,
        to_instability: 0.1,
        suppressed: false,
    }];
    let s = pin("sdp", None);
    mark_sdp_target_suppressions(&mut a, &s);
    assert!(a.sdp_violations[0].suppressed, "sdp target silences SDP");
    // The metric warning is untouched by an sdp target.
    let mut summary = Summary::from_results(&[]);
    count_coupling_warnings(Some(&mut a), &CouplingConfig::default(), &s, &mut summary);
    assert_eq!(
        summary.coupling_warnings, 1,
        "sdp target leaves instability"
    );
}
