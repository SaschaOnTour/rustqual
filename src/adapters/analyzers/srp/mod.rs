pub mod cohesion;
mod collect;
pub mod module;
mod union_find;

pub(crate) use collect::{
    is_self_expr, returns_self, ImplMethodCollector, MethodBodyVisitor, StructCollector,
};

use std::collections::HashSet;

use syn::visit::Visit;

use crate::adapters::shared::file_visitor::{visit_all_files, FileVisitor};
use crate::config::sections::SrpConfig;

/// Warning about a struct that may violate the Single Responsibility Principle.
#[derive(Debug, Clone)]
pub struct SrpWarning {
    pub struct_name: String,
    pub file: String,
    pub line: usize,
    pub lcom4: usize,
    pub field_count: usize,
    pub method_count: usize,
    pub fan_out: usize,
    pub composite_score: f64,
    pub clusters: Vec<ResponsibilityCluster>,
    pub suppressed: bool,
}

/// A cluster of methods that share field accesses (connected component in LCOM4).
#[derive(Debug, Clone)]
pub struct ResponsibilityCluster {
    pub methods: Vec<String>,
    pub fields: Vec<String>,
}

/// Warning about a module with too many production lines or too many independent clusters.
#[derive(Debug, Clone)]
pub struct ModuleSrpWarning {
    pub module: String,
    pub file: String,
    pub production_lines: usize,
    pub length_score: f64,
    /// Number of independent function clusters (0 = not computed or fully connected).
    pub independent_clusters: usize,
    /// Names of functions in each independent cluster.
    pub cluster_names: Vec<Vec<String>>,
    pub suppressed: bool,
}

/// Warning about a function with too many parameters (SRP-004).
#[derive(Debug, Clone)]
pub struct ParamSrpWarning {
    pub function_name: String,
    pub file: String,
    pub line: usize,
    pub parameter_count: usize,
    pub suppressed: bool,
}

/// Complete SRP analysis results.
pub struct SrpAnalysis {
    pub struct_warnings: Vec<SrpWarning>,
    pub module_warnings: Vec<ModuleSrpWarning>,
    pub param_warnings: Vec<ParamSrpWarning>,
}

/// Information about a struct collected from the AST.
pub(crate) struct StructInfo {
    pub name: String,
    pub file: String,
    pub line: usize,
    pub fields: Vec<String>,
    /// Pooling identity: `file::inline_mod_path::name`. Methods pool with a
    /// struct only when their `owner_key` matches, so two same-named structs
    /// in different files / inline modules don't share a method bucket (RQ-1).
    pub owner_key: String,
}

/// Field access and call data for a single method.
pub(crate) struct MethodFieldData {
    pub method_name: String,
    pub parent_type: String,
    /// Pooling identity of the owning type (see [`StructInfo::owner_key`]):
    /// `file::inline_mod_path::parent_type`.
    pub owner_key: String,
    pub field_accesses: HashSet<String>,
    pub call_targets: HashSet<String>,
    /// Method names called on self (e.g. `self.conn()`).
    pub self_method_calls: HashSet<String>,
    /// True if this is a constructor (static method returning Self).
    pub is_constructor: bool,
}

/// Run SRP analysis on all parsed files.
/// Integration: orchestrates struct collection, method data collection,
/// struct-level analysis, and module-level analysis.
pub fn analyze_srp(
    parsed: &[(String, String, syn::File)],
    config: &SrpConfig,
    file_call_graph: &std::collections::HashMap<String, Vec<(String, Vec<String>)>>,
    test_file_length: usize,
) -> SrpAnalysis {
    let mut structs = Vec::new();
    let mut struct_collector = StructCollector {
        file: String::new(),
        module_stack: Vec::new(),
        structs: &mut structs,
    };
    visit_all_files(parsed, &mut struct_collector);

    let mut methods = Vec::new();
    let mut bridges = Vec::new();
    let mut method_collector = ImplMethodCollector {
        file: String::new(),
        module_stack: Vec::new(),
        methods: &mut methods,
        bridges: &mut bridges,
    };
    visit_all_files(parsed, &mut method_collector);

    let struct_warnings = cohesion::build_struct_warnings(&structs, &methods, &bridges, config);
    let cfg_test_files =
        crate::adapters::shared::cfg_test_files::collect_cfg_test_file_paths(parsed);
    let module_warnings = module::analyze_module_srp(
        parsed,
        config,
        file_call_graph,
        &cfg_test_files,
        test_file_length,
    );
    let param_warnings = Vec::new();
    SrpAnalysis {
        struct_warnings,
        module_warnings,
        param_warnings,
    }
}

#[cfg(test)]
mod tests;
