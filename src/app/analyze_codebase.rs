//! Use-case: run every dimension analyzer on one parsed workspace.
//!
//! This is the port-level orchestrator. The composition root picks which
//! concrete analyzers to wire in (architecture, iosp, dry, …); the use-case
//! iterates over them blindly through the `DimensionAnalyzer` trait object
//! and gathers all findings into one flat `Vec`.
//!
//! Adding a dimension is a composition-root change — no edit needed here.

use crate::domain::Finding;
use crate::ports::{AnalysisContext, DimensionAnalyzer};

/// Run every analyzer in `analyzers` against `ctx` and collect findings.
/// Operation: iterator-chain flat-map, no own calls.
pub fn analyze_codebase(
    analyzers: &[Box<dyn DimensionAnalyzer>],
    ctx: &AnalysisContext<'_>,
) -> Vec<Finding> {
    analyzers.iter().flat_map(|a| a.analyze(ctx)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::domain::{Dimension, Severity};
    use crate::ports::ParsedFile;

    struct StubAnalyzer {
        name: &'static str,
        findings: Vec<Finding>,
    }

    impl DimensionAnalyzer for StubAnalyzer {
        fn dimension_name(&self) -> &'static str {
            self.name
        }
        fn analyze(&self, _ctx: &AnalysisContext<'_>) -> Vec<Finding> {
            self.findings.clone()
        }
    }

    fn empty_ctx() -> (Vec<ParsedFile>, Config) {
        (Vec::new(), Config::default())
    }

    fn stub_finding(rule: &str) -> Finding {
        Finding {
            rule_id: rule.to_string(),
            dimension: Dimension::Architecture,
            severity: Severity::Medium,
            ..Finding::default()
        }
    }

    #[test]
    fn empty_analyzer_list_yields_empty() {
        let (files, cfg) = empty_ctx();
        let ctx = AnalysisContext {
            files: &files,
            config: &cfg,
        };
        let out = analyze_codebase(&[], &ctx);
        assert!(out.is_empty());
    }

    #[test]
    fn concatenates_findings_from_each_analyzer() {
        let (files, cfg) = empty_ctx();
        let ctx = AnalysisContext {
            files: &files,
            config: &cfg,
        };
        let a: Box<dyn DimensionAnalyzer> = Box::new(StubAnalyzer {
            name: "a",
            findings: vec![stub_finding("a/x"), stub_finding("a/y")],
        });
        let b: Box<dyn DimensionAnalyzer> = Box::new(StubAnalyzer {
            name: "b",
            findings: vec![stub_finding("b/z")],
        });
        let analyzers: Vec<Box<dyn DimensionAnalyzer>> = vec![a, b];
        let out = analyze_codebase(&analyzers, &ctx);
        let ids: Vec<&str> = out.iter().map(|f| f.rule_id.as_str()).collect();
        assert_eq!(ids, vec!["a/x", "a/y", "b/z"]);
    }

    #[test]
    fn preserves_analyzer_order() {
        let (files, cfg) = empty_ctx();
        let ctx = AnalysisContext {
            files: &files,
            config: &cfg,
        };
        let first: Box<dyn DimensionAnalyzer> = Box::new(StubAnalyzer {
            name: "first",
            findings: vec![stub_finding("first")],
        });
        let second: Box<dyn DimensionAnalyzer> = Box::new(StubAnalyzer {
            name: "second",
            findings: vec![stub_finding("second")],
        });
        let analyzers: Vec<Box<dyn DimensionAnalyzer>> = vec![first, second];
        let out = analyze_codebase(&analyzers, &ctx);
        assert_eq!(out[0].rule_id, "first");
        assert_eq!(out[1].rule_id, "second");
    }
}
