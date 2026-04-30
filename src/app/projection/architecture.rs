//! Architecture-dimension projection: legacy `Vec<Finding>` → typed
//! `Vec<ArchitectureFinding>`.

use crate::domain::findings::ArchitectureFinding;
use crate::domain::Finding;

/// Project legacy architecture findings into typed ArchitectureFinding wrappers.
pub(crate) fn project_architecture(legacy: &[Finding]) -> Vec<ArchitectureFinding> {
    legacy
        .iter()
        .map(|f| ArchitectureFinding { common: f.clone() })
        .collect()
}
