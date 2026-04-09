pub mod init;
pub mod sections;

use globset::GlobSet;
use serde::Deserialize;
use std::path::Path;

pub use init::{generate_default_config, generate_tailored_config};
use sections::DEFAULT_MAX_SUPPRESSION_RATIO;
pub use sections::{
    BoilerplateConfig, ComplexityConfig, CouplingConfig, DuplicatesConfig, SrpConfig,
    StructuralConfig, TestConfig, WeightsConfig,
};

/// Configuration for the rustqual analyzer.
///
/// Can be loaded from a `rustqual.toml` file in the project root.
#[derive(Debug, Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Function name patterns to ignore entirely (e.g. test helpers, macros).
    pub ignore_functions: Vec<String>,

    /// Glob patterns for files to exclude from analysis.
    pub exclude_files: Vec<String>,

    /// Whether to treat closures as logic (strict) or ignore them (lenient).
    pub strict_closures: bool,

    /// Whether iterator adaptor chains (.map, .filter, etc.) count as logic.
    pub strict_iterator_chains: bool,

    /// Whether recursive calls are allowed (don't count as IOSP violations).
    pub allow_recursion: bool,

    /// Whether the `?` operator counts as logic (implicit control flow).
    pub strict_error_propagation: bool,

    /// Maximum ratio of suppressed functions before emitting a warning.
    pub max_suppression_ratio: f64,

    /// If true, exit with code 1 when warnings are present (e.g. suppression ratio exceeded).
    pub fail_on_warnings: bool,

    /// Complexity analysis configuration.
    pub complexity: ComplexityConfig,

    /// Duplicate / DRY detection configuration.
    pub duplicates: DuplicatesConfig,

    /// Boilerplate detection configuration.
    pub boilerplate: BoilerplateConfig,

    /// SRP (Single Responsibility) analysis configuration.
    pub srp: SrpConfig,

    /// Coupling analysis configuration.
    pub coupling: CouplingConfig,

    /// Structural binary checks configuration.
    pub structural: StructuralConfig,

    /// Test quality analysis configuration.
    pub test: TestConfig,

    /// Quality score dimension weights.
    pub weights: WeightsConfig,

    /// Pre-compiled glob set for ignore_functions patterns.
    #[serde(skip)]
    compiled_ignore_fns: Option<GlobSet>,

    /// Pre-compiled glob set for exclude_files patterns.
    #[serde(skip)]
    compiled_exclude_files: Option<GlobSet>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ignore_functions: vec![],
            exclude_files: vec![],
            strict_closures: false,
            strict_iterator_chains: false,
            allow_recursion: false,
            strict_error_propagation: false,
            max_suppression_ratio: DEFAULT_MAX_SUPPRESSION_RATIO,
            fail_on_warnings: false,
            complexity: ComplexityConfig::default(),
            duplicates: DuplicatesConfig::default(),
            boilerplate: BoilerplateConfig::default(),
            srp: SrpConfig::default(),
            coupling: CouplingConfig::default(),
            structural: StructuralConfig::default(),
            test: TestConfig::default(),
            weights: WeightsConfig::default(),
            compiled_ignore_fns: None,
            compiled_exclude_files: None,
        }
    }
}

/// Build a compiled GlobSet from a list of pattern strings.
/// Operation: iterates patterns with error handling logic.
fn build_globset(patterns: &[String]) -> GlobSet {
    let mut builder = globset::GlobSetBuilder::new();
    for pattern in patterns {
        match globset::Glob::new(pattern) {
            Ok(g) => {
                builder.add(g);
            }
            Err(e) => {
                eprintln!("Warning: Invalid glob pattern '{pattern}': {e}");
            }
        }
    }
    builder.build().unwrap_or_else(|_| {
        globset::GlobSetBuilder::new()
            .build()
            .expect("empty GlobSet")
    })
}

/// Check if a target string matches any pattern in a list.
/// Uses pre-compiled GlobSet if available, falls back to per-pattern compilation.
/// Operation: glob matching logic with no own calls.
fn match_any_pattern(patterns: &[String], compiled: &Option<GlobSet>, target: &str) -> bool {
    if let Some(ref gs) = compiled {
        return gs.is_match(target);
    }
    patterns.iter().any(|pattern| {
        if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
            globset::Glob::new(pattern)
                .ok()
                .and_then(|g| g.compile_matcher().is_match(target).then_some(()))
                .is_some()
        } else {
            target == pattern
        }
    })
}

/// The config file name used by rustqual.
const CONFIG_FILE_NAME: &str = "rustqual.toml";

impl Config {
    /// Compile glob patterns into GlobSets for fast matching.
    /// Call this after loading or constructing a Config.
    pub fn compile(&mut self) {
        self.compiled_ignore_fns = Some(build_globset(&self.ignore_functions));
        self.compiled_exclude_files = Some(build_globset(&self.exclude_files));
    }

    /// Try to load configuration from a `rustqual.toml` file.
    /// Searches the given path and its ancestors.
    /// Returns an error if a config file exists but cannot be parsed.
    /// Falls back to defaults if no config file is found.
    pub fn load(project_root: &Path) -> Result<Self, String> {
        let start = if project_root.is_file() {
            project_root.parent().unwrap_or(project_root)
        } else {
            project_root
        };
        let mut dir = Some(start);
        while let Some(d) = dir {
            let config_path = d.join(CONFIG_FILE_NAME);
            if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)
                    .map_err(|e| format!("Failed to read {}: {e}", config_path.display()))?;
                let config: Config = toml::from_str(&content)
                    .map_err(|e| format!("Failed to parse {}: {e}", config_path.display()))?;
                return Ok(config);
            }
            dir = d.parent();
        }
        Ok(Self::default())
    }

    /// Check if a function call path looks like an external/allowed call.
    /// Check if a function name should be ignored (supports full glob patterns).
    /// Trivial: single delegation to match_any_pattern.
    pub fn is_ignored_function(&self, name: &str) -> bool {
        match_any_pattern(&self.ignore_functions, &self.compiled_ignore_fns, name)
    }

    /// Check if a file path matches any exclude_files pattern.
    /// Trivial: single delegation to match_any_pattern.
    pub fn is_excluded_file(&self, path: &str) -> bool {
        match_any_pattern(&self.exclude_files, &self.compiled_exclude_files, path)
    }
}

/// Validate that quality weights sum to approximately 1.0.
/// Operation: arithmetic check with tolerance.
pub fn validate_weights(config: &Config) -> Result<(), String> {
    let w = &config.weights;
    let sum = w.iosp + w.complexity + w.dry + w.srp + w.coupling + w.test;
    if (sum - 1.0).abs() > sections::WEIGHT_SUM_TOLERANCE {
        return Err(format!(
            "Quality weights must sum to 1.0, but sum is {sum:.4}. \
             Check [weights] in rustqual.toml."
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ── is_ignored_function ───────────────────────────────────────────

    #[test]
    fn test_ignored_exact() {
        let cfg = Config {
            ignore_functions: vec!["main".into()],
            ..Config::default()
        };
        assert!(cfg.is_ignored_function("main"));
    }

    #[test]
    fn test_ignored_trailing_glob() {
        let cfg = Config {
            ignore_functions: vec!["test_*".into()],
            ..Config::default()
        };
        assert!(cfg.is_ignored_function("test_foo"));
    }

    #[test]
    fn test_ignored_no_match() {
        let cfg = Config {
            ignore_functions: vec!["test_*".into()],
            ..Config::default()
        };
        assert!(!cfg.is_ignored_function("helper"));
    }

    #[test]
    fn test_ignored_glob_not_prefix() {
        let cfg = Config {
            ignore_functions: vec!["test_*".into()],
            ..Config::default()
        };
        // "my_test" does NOT start with "test_", so it must not match.
        assert!(!cfg.is_ignored_function("my_test"));
    }

    #[test]
    fn test_ignored_compiled_glob() {
        let mut cfg = Config {
            ignore_functions: vec!["test_*".into(), "main".into()],
            ..Config::default()
        };
        cfg.compile();
        assert!(cfg.is_ignored_function("test_foo"));
        assert!(cfg.is_ignored_function("main"));
        assert!(!cfg.is_ignored_function("helper"));
    }

    #[test]
    fn test_excluded_file_compiled() {
        let mut cfg = Config {
            exclude_files: vec!["generated/**".into()],
            ..Config::default()
        };
        cfg.compile();
        assert!(cfg.is_excluded_file("generated/foo.rs"));
        assert!(!cfg.is_excluded_file("src/main.rs"));
    }

    // ── Config::load ──────────────────────────────────────────────────

    #[test]
    fn test_config_loads_rustqual_toml() {
        let tmp = TempDir::new().unwrap();
        let toml_content = r#"
ignore_functions = ["skip_me"]
exclude_files = ["generated/**"]
strict_closures = true
strict_iterator_chains = true
"#;
        fs::write(tmp.path().join("rustqual.toml"), toml_content).unwrap();

        let mut cfg = Config::load(tmp.path()).unwrap();
        assert_eq!(cfg.ignore_functions, vec!["skip_me"]);
        assert_eq!(cfg.exclude_files, vec!["generated/**"]);
        assert!(cfg.strict_closures);
        assert!(cfg.strict_iterator_chains);
        // Globs are compiled on demand via compile()
        cfg.compile();
        assert!(cfg.compiled_ignore_fns.is_some());
        assert!(cfg.compiled_exclude_files.is_some());
    }

    #[test]
    fn test_load_missing_file() {
        let tmp = TempDir::new().unwrap();
        // No rustqual.toml in the temp directory → should fall back to defaults.
        let cfg = Config::load(tmp.path()).unwrap();
        assert!(!cfg.strict_closures);
    }

    #[test]
    fn test_load_invalid_file_returns_error() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("rustqual.toml"),
            "this is not valid toml {{{",
        )
        .unwrap();
        let result = Config::load(tmp.path());
        assert!(result.is_err(), "Invalid TOML should produce an error");
    }

    #[test]
    fn test_load_unknown_field_returns_error() {
        let tmp = TempDir::new().unwrap();
        let toml_content = r#"
strict_closures = false
unknown_field = true
"#;
        fs::write(tmp.path().join("rustqual.toml"), toml_content).unwrap();
        let result = Config::load(tmp.path());
        assert!(
            result.is_err(),
            "Unknown fields should produce an error with deny_unknown_fields"
        );
    }

    #[test]
    fn test_default_values() {
        let cfg = Config::default();
        assert!(!cfg.strict_closures);
        assert!(!cfg.strict_iterator_chains);
        assert!(!cfg.allow_recursion);
        assert!(!cfg.strict_error_propagation);
        assert!(cfg.ignore_functions.is_empty());
    }

    #[test]
    fn test_default_sub_configs() {
        let cfg = Config::default();
        assert!(cfg.complexity.enabled);
        assert!(cfg.duplicates.enabled);
        assert!(cfg.boilerplate.enabled);
        assert!(cfg.srp.enabled);
        assert!(cfg.coupling.enabled);
        assert!((cfg.max_suppression_ratio - DEFAULT_MAX_SUPPRESSION_RATIO).abs() < f64::EPSILON);
    }

    #[test]
    fn test_load_with_sub_configs() {
        let tmp = TempDir::new().unwrap();
        let toml_content = r#"
[complexity]
enabled = false
max_cognitive = 20

[duplicates]
enabled = true
similarity_threshold = 0.90
"#;
        fs::write(tmp.path().join("rustqual.toml"), toml_content).unwrap();
        let cfg = Config::load(tmp.path()).unwrap();
        assert!(!cfg.complexity.enabled);
        assert_eq!(cfg.complexity.max_cognitive, 20);
        assert!(cfg.duplicates.enabled);
        assert!((cfg.duplicates.similarity_threshold - 0.90).abs() < f64::EPSILON);
    }

    #[test]
    fn test_new_fields_default_false() {
        let tmp = TempDir::new().unwrap();
        let toml_content = r#"
strict_closures = false
"#;
        fs::write(tmp.path().join("rustqual.toml"), toml_content).unwrap();
        let cfg = Config::load(tmp.path()).unwrap();
        assert!(!cfg.allow_recursion);
        assert!(!cfg.strict_error_propagation);
    }

    #[test]
    fn test_build_globset_empty() {
        let gs = build_globset(&[]);
        assert!(!gs.is_match("anything"));
    }

    #[test]
    fn test_build_globset_patterns() {
        let gs = build_globset(&["test_*".into(), "main".into()]);
        assert!(gs.is_match("test_foo"));
        assert!(gs.is_match("main"));
        assert!(!gs.is_match("helper"));
    }

    #[test]
    fn test_validate_weights_default_ok() {
        let cfg = Config::default();
        assert!(validate_weights(&cfg).is_ok());
    }

    #[test]
    fn test_validate_weights_bad_sum() {
        let mut cfg = Config::default();
        cfg.weights.iosp = 0.50;
        let result = validate_weights(&cfg);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must sum to 1.0"));
    }
}
