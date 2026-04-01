use super::json_types::{
    JsonModuleSrpWarning, JsonParamSrpWarning, JsonSrp, JsonSrpCluster, JsonSrpWarning,
};

/// Build the JSON SRP section from an SRP analysis.
/// Operation: iteration + filtering + mapping logic, no own calls.
pub(super) fn build_json_srp(srp: &crate::srp::SrpAnalysis) -> JsonSrp {
    JsonSrp {
        struct_warnings: srp
            .struct_warnings
            .iter()
            .filter(|w| !w.suppressed)
            .map(|w| JsonSrpWarning {
                struct_name: w.struct_name.clone(),
                file: w.file.clone(),
                line: w.line,
                lcom4: w.lcom4,
                field_count: w.field_count,
                method_count: w.method_count,
                fan_out: w.fan_out,
                composite_score: w.composite_score,
                clusters: w
                    .clusters
                    .iter()
                    .map(|c| JsonSrpCluster {
                        methods: c.methods.clone(),
                        fields: c.fields.clone(),
                    })
                    .collect(),
            })
            .collect(),
        module_warnings: srp
            .module_warnings
            .iter()
            .filter(|w| !w.suppressed)
            .map(|w| JsonModuleSrpWarning {
                module: w.module.clone(),
                file: w.file.clone(),
                production_lines: w.production_lines,
                length_score: w.length_score,
                independent_clusters: w.independent_clusters,
                cluster_names: w.cluster_names.clone(),
            })
            .collect(),
        param_warnings: srp
            .param_warnings
            .iter()
            .filter(|w| !w.suppressed)
            .map(|w| JsonParamSrpWarning {
                function_name: w.function_name.clone(),
                file: w.file.clone(),
                line: w.line,
                parameter_count: w.parameter_count,
            })
            .collect(),
    }
}
