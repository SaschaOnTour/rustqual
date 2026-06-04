//! Generic per-file AST visitor infrastructure shared across analyzers.
//!
//! A small utility (no analyzer dependency) so any dimension can walk every
//! parsed file with a stateful `syn` visitor, resetting per-file state between
//! files. Lives in `shared/` so analyzers consume it directly rather than
//! reaching into one another.

use syn::visit::Visit;

/// Trait for AST visitors that need per-file state reset.
pub(crate) trait FileVisitor {
    fn reset_for_file(&mut self, file_path: &str);
}

/// Visit all parsed files with a visitor, resetting per-file state.
/// Trivial: iteration with trait method call.
pub(crate) fn visit_all_files<'a, V>(parsed: &'a [(String, String, syn::File)], visitor: &mut V)
where
    V: FileVisitor + Visit<'a>,
{
    parsed.iter().for_each(|(path, _, file)| {
        visitor.reset_for_file(path);
        syn::visit::visit_file(visitor, file);
    });
}
