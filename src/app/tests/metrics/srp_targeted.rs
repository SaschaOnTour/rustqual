//! Target-aware SRP module suppression: a `file_length` pin silences the
//! length warning only while `production_lines <= pin` (re-firing above), and
//! never hides a co-occurring cohesion finding. Blanket `allow(srp)` still
//! silences everything.
use super::*;
use crate::adapters::analyzers::srp::{ModuleSrpWarning, SrpAnalysis};
use crate::domain::SuppressionTarget;
use crate::findings::Dimension;
use std::collections::HashMap;

fn module_warning(production_lines: usize, length_score: f64, clusters: usize) -> ModuleSrpWarning {
    ModuleSrpWarning {
        module: "m.rs".to_string(),
        file: "m.rs".to_string(),
        production_lines,
        length_score,
        independent_clusters: clusters,
        cluster_names: vec![],
        suppressed: false,
    }
}

fn sups(target: Option<SuppressionTarget>) -> HashMap<String, Vec<Suppression>> {
    [(
        "m.rs".to_string(),
        vec![Suppression {
            line: 1,
            dimensions: vec![Dimension::Srp],
            reason: Some("r".to_string()),
            target,
        }],
    )]
    .into()
}

fn pin(name: &str, pin: Option<f64>) -> HashMap<String, Vec<Suppression>> {
    sups(Some(SuppressionTarget {
        name: name.to_string(),
        pin,
    }))
}

fn suppressed(w: ModuleSrpWarning, s: &HashMap<String, Vec<Suppression>>) -> bool {
    let mut srp = SrpAnalysis {
        struct_warnings: vec![],
        module_warnings: vec![w],
        param_warnings: vec![],
    };
    mark_srp_suppressions(Some(&mut srp), s, &SrpConfig::default());
    srp.module_warnings[0].suppressed
}

#[test]
fn file_length_pin_suppresses_within_pin() {
    // 350 lines (threshold 300 → score 1.167), pin 400 → 350 <= 400 → silenced.
    let w = module_warning(350, 350.0 / 300.0, 0);
    assert!(suppressed(w, &pin("file_length", Some(400.0))));
}

#[test]
fn file_length_pin_refires_above_pin() {
    // 450 lines, pin 400 → 450 > 400 → the warning re-fires (not suppressed).
    let w = module_warning(450, 450.0 / 300.0, 0);
    assert!(!suppressed(w, &pin("file_length", Some(400.0))));
}

#[test]
fn file_length_pin_does_not_hide_cohesion() {
    // Length fires (350 lines) AND cohesion fires (5 clusters > max 2). A
    // file_length pin covers only the length component, so the warning stays.
    let w = module_warning(350, 350.0 / 300.0, 5);
    assert!(!suppressed(w, &pin("file_length", Some(400.0))));
}

#[test]
fn blanket_allow_srp_suppresses_module() {
    let w = module_warning(350, 350.0 / 300.0, 5);
    assert!(suppressed(w, &sups(None)));
}

#[test]
fn cluster_pin_suppresses_cohesion_only_warning() {
    // Length under threshold (score 0.5), 3 clusters > max 2, pin clusters=4.
    let w = module_warning(150, 0.5, 3);
    assert!(suppressed(w, &pin("max_independent_clusters", Some(4.0))));
}

#[test]
fn wrong_target_does_not_suppress() {
    // A max_parameters pin must not touch a module length warning.
    let w = module_warning(450, 450.0 / 300.0, 0);
    assert!(!suppressed(w, &pin("max_parameters", Some(999.0))));
}
