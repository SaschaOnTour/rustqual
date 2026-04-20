use serde::Deserialize;

// ── Default constants (configurable thresholds) ─────────────────────────

pub const DEFAULT_MAX_SUPPRESSION_RATIO: f64 = 0.05;

// Complexity
pub const DEFAULT_COMPLEXITY_ENABLED: bool = true;
pub const DEFAULT_MAX_COGNITIVE: usize = 15;
pub const DEFAULT_MAX_CYCLOMATIC: usize = 10;
pub const DEFAULT_MAX_NESTING_DEPTH: usize = 4;
pub const DEFAULT_MAX_FUNCTION_LINES: usize = 60;
pub const DEFAULT_DETECT_MAGIC_NUMBERS: bool = true;
pub const DEFAULT_DETECT_UNSAFE: bool = true;
pub const DEFAULT_DETECT_ERROR_HANDLING: bool = true;
pub const DEFAULT_ALLOW_EXPECT: bool = false;

// DRY / Duplicates
pub const DEFAULT_DUPLICATES_ENABLED: bool = true;
pub const DEFAULT_DETECT_WILDCARD_IMPORTS: bool = true;
pub const DEFAULT_DETECT_REPEATED_MATCHES: bool = true;
pub const DEFAULT_SIMILARITY_THRESHOLD: f64 = 0.85;
pub const DEFAULT_MIN_TOKENS: usize = 30;
pub const DEFAULT_MIN_LINES: usize = 5;
pub const DEFAULT_MIN_STATEMENTS: usize = 3;
pub const DEFAULT_DETECT_DEAD_CODE: bool = true;

// Boilerplate
pub const DEFAULT_BOILERPLATE_ENABLED: bool = true;

// SRP
pub const DEFAULT_SRP_ENABLED: bool = true;
pub const DEFAULT_SRP_SMELL_THRESHOLD: f64 = 0.6;
pub const DEFAULT_SRP_MAX_FIELDS: usize = 12;
pub const DEFAULT_SRP_MAX_METHODS: usize = 20;
pub const DEFAULT_SRP_MAX_FAN_OUT: usize = 10;
pub const DEFAULT_SRP_LCOM4_THRESHOLD: usize = 2;
pub const DEFAULT_SRP_FILE_LENGTH_BASELINE: usize = 300;
pub const DEFAULT_SRP_FILE_LENGTH_CEILING: usize = 800;
// Highest cluster count that still passes. A file with more than
// this many independent function clusters is flagged as having
// multiple responsibilities. Default `2` means 3+ clusters trigger
// the warning — preserving the historical threshold now that the
// comparison uses strict `>` consistent with other `max_*` fields.
pub const DEFAULT_SRP_MAX_INDEPENDENT_CLUSTERS: usize = 2;
pub const DEFAULT_SRP_MIN_CLUSTER_STATEMENTS: usize = 5;
pub const DEFAULT_SRP_MAX_PARAMETERS: usize = 5;

// Coupling
pub const DEFAULT_COUPLING_ENABLED: bool = true;
pub const DEFAULT_CHECK_SDP: bool = true;
pub const DEFAULT_MAX_INSTABILITY: f64 = 0.8;
pub const DEFAULT_MAX_FAN_IN: usize = 15;
pub const DEFAULT_MAX_FAN_OUT_COUPLING: usize = 12;

// Structural (binary checks)
pub const DEFAULT_STRUCTURAL_ENABLED: bool = true;

// Test Quality
pub const DEFAULT_TEST_ENABLED: bool = true;

// Quality weights: [IOSP, Complexity, DRY, SRP, Coupling, TestQuality, Architecture]
pub const DEFAULT_QUALITY_WEIGHTS: [f64; 7] = [0.22, 0.18, 0.13, 0.18, 0.09, 0.10, 0.10];

/// Maximum acceptable deviation from 1.0 for weight sum validation.
pub const WEIGHT_SUM_TOLERANCE: f64 = 0.001;

// ── Sub-config structs ──────────────────────────────────────────────────

/// Configuration for complexity analysis.
#[derive(Debug, Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct ComplexityConfig {
    pub enabled: bool,
    pub max_cognitive: usize,
    pub max_cyclomatic: usize,
    pub max_nesting_depth: usize,
    pub max_function_lines: usize,
    pub include_nesting_penalty: bool,
    pub detect_magic_numbers: bool,
    pub detect_unsafe: bool,
    pub detect_error_handling: bool,
    pub allow_expect: bool,
    pub allowed_magic_numbers: Vec<String>,
}

impl Default for ComplexityConfig {
    fn default() -> Self {
        Self {
            enabled: DEFAULT_COMPLEXITY_ENABLED,
            max_cognitive: DEFAULT_MAX_COGNITIVE,
            max_cyclomatic: DEFAULT_MAX_CYCLOMATIC,
            max_nesting_depth: DEFAULT_MAX_NESTING_DEPTH,
            max_function_lines: DEFAULT_MAX_FUNCTION_LINES,
            include_nesting_penalty: true,
            detect_magic_numbers: DEFAULT_DETECT_MAGIC_NUMBERS,
            detect_unsafe: DEFAULT_DETECT_UNSAFE,
            detect_error_handling: DEFAULT_DETECT_ERROR_HANDLING,
            allow_expect: DEFAULT_ALLOW_EXPECT,
            allowed_magic_numbers: vec![
                "0".into(),
                "1".into(),
                "-1".into(),
                "2".into(),
                "0.0".into(),
                "1.0".into(),
            ],
        }
    }
}

/// Configuration for duplicate / DRY detection.
#[derive(Debug, Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct DuplicatesConfig {
    pub enabled: bool,
    pub similarity_threshold: f64,
    pub min_tokens: usize,
    pub min_lines: usize,
    pub min_statements: usize,
    pub ignore_tests: bool,
    pub ignore_trait_impls: bool,
    pub detect_dead_code: bool,
    pub detect_wildcard_imports: bool,
    pub detect_repeated_matches: bool,
}

impl Default for DuplicatesConfig {
    fn default() -> Self {
        Self {
            enabled: DEFAULT_DUPLICATES_ENABLED,
            similarity_threshold: DEFAULT_SIMILARITY_THRESHOLD,
            min_tokens: DEFAULT_MIN_TOKENS,
            min_lines: DEFAULT_MIN_LINES,
            min_statements: DEFAULT_MIN_STATEMENTS,
            ignore_tests: true,
            ignore_trait_impls: true,
            detect_dead_code: DEFAULT_DETECT_DEAD_CODE,
            detect_wildcard_imports: DEFAULT_DETECT_WILDCARD_IMPORTS,
            detect_repeated_matches: DEFAULT_DETECT_REPEATED_MATCHES,
        }
    }
}

/// Configuration for boilerplate detection.
#[derive(Debug, Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct BoilerplateConfig {
    pub enabled: bool,
    pub patterns: Vec<String>,
    pub suggest_crates: bool,
}

impl Default for BoilerplateConfig {
    fn default() -> Self {
        Self {
            enabled: DEFAULT_BOILERPLATE_ENABLED,
            patterns: vec![],
            suggest_crates: true,
        }
    }
}

/// Configuration for SRP (Single Responsibility Principle) analysis.
#[derive(Debug, Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct SrpConfig {
    pub enabled: bool,
    pub smell_threshold: f64,
    pub max_fields: usize,
    pub max_methods: usize,
    pub max_fan_out: usize,
    pub lcom4_threshold: usize,
    pub weights: [f64; 4],
    pub file_length_baseline: usize,
    pub file_length_ceiling: usize,
    pub max_independent_clusters: usize,
    pub min_cluster_statements: usize,
    pub max_parameters: usize,
}

impl Default for SrpConfig {
    fn default() -> Self {
        Self {
            enabled: DEFAULT_SRP_ENABLED,
            smell_threshold: DEFAULT_SRP_SMELL_THRESHOLD,
            max_fields: DEFAULT_SRP_MAX_FIELDS,
            max_methods: DEFAULT_SRP_MAX_METHODS,
            max_fan_out: DEFAULT_SRP_MAX_FAN_OUT,
            lcom4_threshold: DEFAULT_SRP_LCOM4_THRESHOLD,
            weights: [0.4, 0.25, 0.15, 0.2],
            file_length_baseline: DEFAULT_SRP_FILE_LENGTH_BASELINE,
            file_length_ceiling: DEFAULT_SRP_FILE_LENGTH_CEILING,
            max_independent_clusters: DEFAULT_SRP_MAX_INDEPENDENT_CLUSTERS,
            min_cluster_statements: DEFAULT_SRP_MIN_CLUSTER_STATEMENTS,
            max_parameters: DEFAULT_SRP_MAX_PARAMETERS,
        }
    }
}

/// Configuration for coupling analysis.
#[derive(Debug, Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct CouplingConfig {
    pub enabled: bool,
    pub max_instability: f64,
    pub max_fan_in: usize,
    pub max_fan_out: usize,
    pub check_sdp: bool,
}

impl Default for CouplingConfig {
    fn default() -> Self {
        Self {
            enabled: DEFAULT_COUPLING_ENABLED,
            max_instability: DEFAULT_MAX_INSTABILITY,
            max_fan_in: DEFAULT_MAX_FAN_IN,
            max_fan_out: DEFAULT_MAX_FAN_OUT_COUPLING,
            check_sdp: DEFAULT_CHECK_SDP,
        }
    }
}

/// Configuration for structural binary checks.
#[derive(Debug, Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct StructuralConfig {
    pub enabled: bool,
    pub check_btc: bool,
    pub check_slm: bool,
    pub check_nms: bool,
    pub check_oi: bool,
    pub check_sit: bool,
    pub check_deh: bool,
    pub check_iet: bool,
}

impl Default for StructuralConfig {
    fn default() -> Self {
        Self {
            enabled: DEFAULT_STRUCTURAL_ENABLED,
            check_btc: true,
            check_slm: true,
            check_nms: true,
            check_oi: true,
            check_sit: true,
            check_deh: true,
            check_iet: true,
        }
    }
}

/// Configuration for test quality analysis.
#[derive(Debug, Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct TestConfig {
    pub enabled: bool,
    /// Optional path to an LCOV coverage file for TQ-004/TQ-005 checks.
    pub coverage_file: Option<String>,
    /// Extra macro names (beyond `assert*`) to recognize as assertions in TQ-001.
    pub extra_assertion_macros: Vec<String>,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            enabled: DEFAULT_TEST_ENABLED,
            coverage_file: None,
            extra_assertion_macros: vec![],
        }
    }
}

/// Configuration for rustqual-wide report aggregation (used in workspace mode).
///
/// Applies to all dimensions, not architecture-specific.
#[derive(Debug, Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct ReportConfig {
    /// Aggregation strategy across workspace member crates.
    /// Allowed values: "loc_weighted" | "arithmetic".
    pub aggregation: String,
}

impl Default for ReportConfig {
    fn default() -> Self {
        Self {
            aggregation: "loc_weighted".to_string(),
        }
    }
}

/// Configuration for quality score dimension weights.
#[derive(Debug, Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct WeightsConfig {
    pub iosp: f64,
    pub complexity: f64,
    pub dry: f64,
    pub srp: f64,
    pub coupling: f64,
    pub test_quality: f64,
    pub architecture: f64,
}

impl WeightsConfig {
    /// Convert weights to an array in the standard dimension order.
    /// Operation: trivial field access.
    pub fn as_array(&self) -> [f64; 7] {
        [
            self.iosp,
            self.complexity,
            self.dry,
            self.srp,
            self.coupling,
            self.test_quality,
            self.architecture,
        ]
    }
}

impl Default for WeightsConfig {
    fn default() -> Self {
        let [iosp, complexity, dry, srp, coupling, test_quality, architecture] =
            DEFAULT_QUALITY_WEIGHTS;
        Self {
            iosp,
            complexity,
            dry,
            srp,
            coupling,
            test_quality,
            architecture,
        }
    }
}
