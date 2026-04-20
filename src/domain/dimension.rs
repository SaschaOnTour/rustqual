//! Quality dimensions that rustqual analyzes.
//!
//! Each dimension is a self-contained aspect of code quality, independently
//! weighted in the overall score. `Dimension` is the shared identifier used
//! by findings, suppressions, rules and report output.

/// One of the seven analysis dimensions of rustqual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dimension {
    /// Integration/Operation Segregation Principle.
    Iosp,
    /// Cognitive/cyclomatic complexity, nesting, length, magic numbers.
    Complexity,
    /// Duplicate and boilerplate detection.
    Dry,
    /// Single Responsibility Principle: cohesion, module length, param count.
    Srp,
    /// Afferent/efferent coupling, instability, circular dependencies, SDP.
    Coupling,
    /// Assertion density, untested code, coverage gaps.
    TestQuality,
    /// Layer/forbidden-edge/symbol-policy/trait-contract enforcement (v1.0).
    Architecture,
}

impl Dimension {
    /// Parse a dimension name (case-insensitive).
    ///
    /// Accepted spellings:
    /// - `iosp`, `complexity`, `dry`, `srp`, `coupling`, `architecture`
    /// - `test_quality` (canonical), `tq` (short form), `test` (legacy)
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "iosp" => Some(Self::Iosp),
            "complexity" => Some(Self::Complexity),
            "dry" => Some(Self::Dry),
            "srp" => Some(Self::Srp),
            "coupling" => Some(Self::Coupling),
            "test_quality" | "tq" | "test" => Some(Self::TestQuality),
            "architecture" => Some(Self::Architecture),
            _ => None,
        }
    }
}

impl std::fmt::Display for Dimension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Iosp => write!(f, "iosp"),
            Self::Complexity => write!(f, "complexity"),
            Self::Dry => write!(f, "dry"),
            Self::Srp => write!(f, "srp"),
            Self::Coupling => write!(f, "coupling"),
            Self::TestQuality => write!(f, "test_quality"),
            Self::Architecture => write!(f, "architecture"),
        }
    }
}
