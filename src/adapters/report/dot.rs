//! Graphviz DOT call-graph visualisation.
//!
//! `DotReporter` renders the per-function call graph as a DOT diagram —
//! nodes coloured by IOSP classification, edges from each function's
//! own-calls. Data-only reporter: every finding view is `()` because
//! dot's user (someone piping into `dot -Tsvg`) doesn't want findings
//! cluttering the graph; only the IOSP function records become nodes
//! and edges.
//!
//! Pure-data Views: `build_iosp_data` projects FunctionRecords into
//! typed `DotNode`/`DotEdge` rows; `publish` renders them to DOT
//! syntax.

use crate::domain::analysis_data::{FunctionClassification, FunctionRecord, ModuleCouplingRecord};
use crate::domain::findings::{
    ArchitectureFinding, ComplexityFinding, CouplingFinding, DryFinding, IospFinding,
    OrphanSuppression, SrpFinding, TqFinding,
};
use crate::domain::{AnalysisData, AnalysisFindings};
use crate::ports::reporter::{ReporterImpl, Snapshot};
use crate::ports::Reporter;

pub struct DotReporter;

/// One graph-node, structured (no DOT syntax yet).
pub struct DotNode {
    pub qualified_name: String,
    pub classification: FunctionClassification,
}

/// One graph-edge.
pub struct DotEdge {
    pub from: String,
    pub to: String,
}

/// IOSP data view: nodes + edges, structured.
pub struct DotIospDataView {
    pub nodes: Vec<DotNode>,
    pub edges: Vec<DotEdge>,
}

impl ReporterImpl for DotReporter {
    type Output = String;

    type IospView = ();
    type ComplexityView = ();
    type DryView = ();
    type SrpView = ();
    type CouplingView = ();
    type TestQualityView = ();
    type ArchitectureView = ();
    type OrphanView = ();
    type IospDataView = DotIospDataView;
    type ComplexityDataView = ();
    type CouplingDataView = ();

    fn build_iosp(&self, _: &[IospFinding]) {}
    fn build_complexity(&self, _: &[ComplexityFinding]) {}
    fn build_dry(&self, _: &[DryFinding]) {}
    fn build_srp(&self, _: &[SrpFinding]) {}
    fn build_coupling(&self, _: &[CouplingFinding]) {}
    fn build_test_quality(&self, _: &[TqFinding]) {}
    fn build_architecture(&self, _: &[ArchitectureFinding]) {}
    fn build_orphans(&self, _: &[OrphanSuppression]) {}

    /// IOSP data view: project function records into nodes + edges.
    fn build_iosp_data(&self, functions: &[FunctionRecord]) -> DotIospDataView {
        let visible: Vec<&FunctionRecord> = functions.iter().filter(|f| !f.suppressed).collect();
        let nodes = visible
            .iter()
            .map(|f| DotNode {
                qualified_name: f.qualified_name.clone(),
                classification: f.classification,
            })
            .collect();
        let edges = visible
            .iter()
            .flat_map(|f| {
                f.own_calls.iter().map(move |callee| DotEdge {
                    from: f.qualified_name.clone(),
                    to: callee.clone(),
                })
            })
            .collect();
        DotIospDataView { nodes, edges }
    }

    fn build_complexity_data(&self, _fns: &[FunctionRecord]) {}
    fn build_coupling_data(&self, _mods: &[ModuleCouplingRecord]) {}

    fn publish(&self, snapshot: Snapshot<Self>) -> String {
        let Snapshot {
            iosp: (),
            complexity: (),
            dry: (),
            srp: (),
            coupling: (),
            test_quality: (),
            architecture: (),
            orphans: (),
            iosp_data,
            complexity_data: (),
            coupling_data: (),
        } = snapshot;
        const HEADER: &str = "digraph rustqual {\n  rankdir=LR;\n  \
                              node [shape=box, style=filled, fontname=\"Helvetica\"];\n";
        const FOOTER: &str = "}\n";
        let mut out = String::new();
        out.push_str(HEADER);
        iosp_data
            .nodes
            .iter()
            .for_each(|n| out.push_str(&format_node(n)));
        iosp_data
            .edges
            .iter()
            .for_each(|e| out.push_str(&format_edge(e)));
        out.push_str(FOOTER);
        out
    }
}

fn sanitize(name: &str) -> String {
    name.replace("::", "_")
        .replace(['.', '<', '>', ','], "_")
        .replace(['(', ')'], "")
}

fn classification_colours(c: FunctionClassification) -> (&'static str, &'static str) {
    match c {
        FunctionClassification::Integration => ("#c8e6c9", "#1b5e20"),
        FunctionClassification::Operation => ("#bbdefb", "#0d47a1"),
        FunctionClassification::Trivial => ("#f5f5f5", "#9e9e9e"),
        FunctionClassification::Violation => ("#ffcdd2", "#b71c1c"),
    }
}

fn node_id(name: &str) -> String {
    format!("\"{}\"", sanitize(name))
}

fn format_node(n: &DotNode) -> String {
    let id = node_id(&n.qualified_name);
    let (color, fontcolor) = classification_colours(n.classification);
    format!(
        "  {id} [label=\"{}\", fillcolor=\"{color}\", fontcolor=\"{fontcolor}\"];\n",
        n.qualified_name,
    )
}

fn format_edge(e: &DotEdge) -> String {
    let from = node_id(&e.from);
    let to = node_id(&e.to);
    format!("  {from} -> {to};\n")
}

/// Print analysis results as Graphviz DOT format for call-graph
/// visualisation. Thin wrapper around `DotReporter.render(...)`; uses
/// an empty `AnalysisFindings` because dot doesn't read findings.
pub fn print_dot(data: &AnalysisData) {
    let findings = AnalysisFindings::default();
    print!("{}", DotReporter.render(&findings, data));
}
