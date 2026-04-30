//! `JsonChunk` — the per-Reporter-method Output type used by the JSON
//! reporter. Each method's chunk populates only the sections it
//! contributes to; the orchestrator merges all 7 finding chunks plus
//! 3 data chunks into a single fully-populated chunk that becomes the
//! JsonOutput envelope.

use super::super::json_types::{
    JsonArchitectureFinding, JsonBoilerplateFind, JsonCouplingModule, JsonDeadCodeWarning,
    JsonDuplicateGroup, JsonFragmentGroup, JsonFunction, JsonModuleSrpWarning, JsonParamSrpWarning,
    JsonRepeatedMatchGroup, JsonSdpViolation, JsonSrpWarning, JsonStructuralWarning, JsonTqWarning,
    JsonWildcardWarning,
};

/// All sections any per-dimension JsonReporter method might populate.
/// Empty Vec is the canonical "I don't contribute here" value.
#[derive(Default)]
pub struct JsonChunk {
    pub functions: Vec<JsonFunction>,
    pub coupling_modules: Vec<JsonCouplingModule>,
    pub cycles: Vec<Vec<String>>,
    pub sdp_violations: Vec<JsonSdpViolation>,
    pub duplicates: Vec<JsonDuplicateGroup>,
    pub dead_code: Vec<JsonDeadCodeWarning>,
    pub fragments: Vec<JsonFragmentGroup>,
    pub wildcards: Vec<JsonWildcardWarning>,
    pub boilerplate: Vec<JsonBoilerplateFind>,
    pub repeated_matches: Vec<JsonRepeatedMatchGroup>,
    pub srp_struct: Vec<JsonSrpWarning>,
    pub srp_module: Vec<JsonModuleSrpWarning>,
    pub srp_param: Vec<JsonParamSrpWarning>,
    pub structural: Vec<JsonStructuralWarning>,
    pub tq_warnings: Vec<JsonTqWarning>,
    pub architecture: Vec<JsonArchitectureFinding>,
}

impl JsonChunk {
    /// Merge another chunk's section contributions into self.
    pub fn extend_from(&mut self, other: JsonChunk) {
        self.functions.extend(other.functions);
        self.coupling_modules.extend(other.coupling_modules);
        self.cycles.extend(other.cycles);
        self.sdp_violations.extend(other.sdp_violations);
        self.duplicates.extend(other.duplicates);
        self.dead_code.extend(other.dead_code);
        self.fragments.extend(other.fragments);
        self.wildcards.extend(other.wildcards);
        self.boilerplate.extend(other.boilerplate);
        self.repeated_matches.extend(other.repeated_matches);
        self.srp_struct.extend(other.srp_struct);
        self.srp_module.extend(other.srp_module);
        self.srp_param.extend(other.srp_param);
        self.structural.extend(other.structural);
        self.tq_warnings.extend(other.tq_warnings);
        self.architecture.extend(other.architecture);
    }
}
