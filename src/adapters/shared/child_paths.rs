//! Resolving `mod name;` declarations to the file that implements them.
//!
//! One place owns rustc's file-layout rules. A module's children live in its
//! *module directory*: the declaring file's directory when that file is
//! `mod.rs` / `lib.rs` / `main.rs`, otherwise a directory named after the
//! file's stem — extended by every inline `mod {}` block between the file's
//! top level and the declaration. `mod name;` is then `{dir}/{name}.rs` or
//! `{dir}/{name}/mod.rs`.
//!
//! `#[path = "…"]` overrides the name but not the base, and its base is the
//! one place the two chains differ: at a file's top level it is relative to
//! the *file's own directory*, inside an inline block to that block's module
//! directory. `..` and `.` segments are resolved, since the result is compared
//! against recorded paths as a string.
//!
//! Every pass that needs the module tree (cfg-test classification, external
//! reachability) resolves through this, so the rules cannot drift apart.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The parsed set as `(path, ast)` pairs.
pub(crate) type ParsedRefs<'a> = [(&'a str, &'a syn::File)];

/// Resolves `mod name;` declarations to child file paths by probing the
/// candidate `{parent_dir}/{name}.rs` and `{parent_dir}/{name}/mod.rs`
/// locations against the set of known file paths.
pub(crate) struct ChildPathResolver<'a> {
    known_paths: HashSet<&'a str>,
}

impl<'a> ChildPathResolver<'a> {
    pub(crate) fn from_parsed(parsed: &'a ParsedRefs<'a>) -> Self {
        Self {
            known_paths: parsed.iter().map(|(p, _)| *p).collect(),
        }
    }

    /// Resolve one `mod name;`. `inline_mods` names the inline `mod {}` blocks
    /// between the declaring file's top level and this declaration, outermost
    /// first — empty for a declaration at the file's top level.
    /// Integration: dispatch on the presence of a `#[path]`.
    pub(crate) fn resolve(
        &self,
        parent_path: &str,
        inline_mods: &[String],
        mod_item: &syn::ItemMod,
    ) -> Option<String> {
        match path_attribute(&mod_item.attrs) {
            Some(explicit) => self.resolve_explicit_path(parent_path, inline_mods, &explicit),
            None => {
                self.resolve_by_convention(parent_path, inline_mods, &mod_item.ident.to_string())
            }
        }
    }

    /// `#[path = "custom.rs"]` against the base rustc uses for its position.
    /// Integration: base + join, existence check delegated.
    fn resolve_explicit_path(
        &self,
        parent_path: &str,
        inline_mods: &[String],
        relative: &str,
    ) -> Option<String> {
        let base = explicit_path_base(parent_path, inline_mods);
        self.first_known(&[join_lexical(&base, relative)])
    }

    /// Naming-convention resolution: try `{dir}/{name}.rs` then
    /// `{dir}/{name}/mod.rs` under the declaring module's directory.
    /// Integration: candidate construction, existence check delegated.
    fn resolve_by_convention(
        &self,
        parent_path: &str,
        inline_mods: &[String],
        mod_name: &str,
    ) -> Option<String> {
        let dir = module_dir(parent_path, inline_mods);
        self.first_known(&[
            join_lexical(&dir, &format!("{mod_name}.rs")),
            join_lexical(&dir.join(mod_name), "mod.rs"),
        ])
    }

    /// The first candidate that names a file in the parsed set.
    /// Operation: lookup over the candidates, no own calls.
    fn first_known(&self, candidates: &[String]) -> Option<String> {
        candidates
            .iter()
            .find(|c| self.known_paths.contains(c.as_str()))
            .cloned()
    }
}

/// The directory a module's children live in: the declaring file's module
/// directory, extended by the inline `mod {}` blocks around the declaration.
/// `mod.rs` / `lib.rs` / `main.rs` keep their own directory; any other file
/// opens one named after its stem.
/// Operation: path arithmetic, no own calls.
fn module_dir(parent_path: &str, inline_mods: &[String]) -> PathBuf {
    let parent = Path::new(parent_path);
    let opens_own_dir = !parent
        .file_stem()
        .is_some_and(|s| s == "mod" || s == "lib" || s == "main");
    let mut dir = match opens_own_dir {
        true => parent.with_extension(""),
        false => parent.parent().unwrap_or(Path::new("")).to_path_buf(),
    };
    dir.extend(inline_mods);
    dir
}

/// The base a `#[path]` resolves against: the declaring file's own directory
/// at its top level, the surrounding inline block's module directory inside
/// one. The two coincide only for `mod.rs`-style files.
/// Operation: selects between two bases, own call hidden in the closure.
fn explicit_path_base(parent_path: &str, inline_mods: &[String]) -> PathBuf {
    let file_dir = Path::new(parent_path)
        .parent()
        .unwrap_or(Path::new(""))
        .to_path_buf();
    inline_mods
        .first()
        .map_or(file_dir, |_| module_dir(parent_path, inline_mods))
}

/// Join and resolve `.` / `..` textually: the result is compared against
/// recorded paths as a string, and `src/a/../shared/api.rs` never equals
/// `src/shared/api.rs`. Leading `..` segments are kept — dropping them could
/// fold a path that escapes the analysis root onto an unrelated file inside
/// it. Also normalises the separators `Path::join` produces on Windows.
/// Operation: fold over the segments, no own calls.
fn join_lexical(base: &Path, relative: &str) -> String {
    let joined = base.join(relative).to_string_lossy().replace('\\', "/");
    let mut out: Vec<&str> = Vec::new();
    for segment in joined.split('/') {
        match segment {
            "" | "." => {}
            ".." if matches!(out.last(), Some(last) if *last != "..") => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out.join("/")
}

/// Extract the string value of a `#[path = "..."]` attribute if present.
/// Operation: attribute lookup + literal parsing, no own calls.
fn path_attribute(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("path") {
            return None;
        }
        match &attr.meta {
            syn::Meta::NameValue(nv) => match &nv.value {
                syn::Expr::Lit(expr_lit) => match &expr_lit.lit {
                    syn::Lit::Str(s) => Some(s.value()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    })
}
