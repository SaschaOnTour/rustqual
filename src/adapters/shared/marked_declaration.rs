//! Attaching the bare markers (`// qual:api`, `// qual:test_helper`) to a
//! declaration.
//!
//! Both DRY-002 (functions) and DRY-006 (types and constants) attach markers by
//! the same line-proximity rule, and the stale-marker check judges the result.
//! If the three ever disagreed, a working marker would be reported stale — so
//! the window lives in one place and both declaration kinds go through it.

use std::collections::{HashMap, HashSet};

/// Marker lines per file, as collected from the source.
pub(crate) type MarkerLines = HashMap<String, HashSet<usize>>;

/// A declaration the bare markers can attach to.
pub(crate) trait MarkedDeclaration {
    fn file(&self) -> &str;
    fn line(&self) -> usize;
}

/// Apply `mark` to every declaration with a marker in its annotation window.
/// Operation: iteration + window test, own calls hidden in the closure.
pub(crate) fn mark_annotated<T: MarkedDeclaration>(
    declared: &mut [T],
    marker_lines: &MarkerLines,
    mark: fn(&mut T),
) {
    declared.iter_mut().for_each(|d| {
        let in_window = marker_lines
            .get(d.file())
            .is_some_and(|lines| crate::findings::has_annotation_in_window(lines, d.line()));
        if in_window {
            mark(d);
        }
    });
}

impl MarkedDeclaration for super::declared_function::DeclaredFunction {
    fn file(&self) -> &str {
        &self.file
    }
    fn line(&self) -> usize {
        self.line
    }
}

impl MarkedDeclaration for super::declared_type::DeclaredType {
    fn file(&self) -> &str {
        &self.file
    }
    fn line(&self) -> usize {
        self.line
    }
}
