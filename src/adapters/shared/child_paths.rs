//! Resolving `mod name;` declarations to the file that implements them.
//!
//! One place owns rustc's file-layout rules — `#[path = "…"]` relative to the
//! declaring file's directory, otherwise `{dir}/{name}.rs` or
//! `{dir}/{name}/mod.rs` under the parent's module directory, with `mod.rs` /
//! `lib.rs` / `main.rs` keeping the parent's directory rather than opening a
//! new one. Every pass that needs the module tree (cfg-test classification,
//! external reachability) resolves through this, so the rules cannot drift
//! apart between them.

use std::borrow::Cow;
use std::collections::HashSet;
use std::path::Path;

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

    pub(crate) fn resolve(&self, parent_path: &str, mod_item: &syn::ItemMod) -> Option<String> {
        if let Some(explicit) = path_attribute(&mod_item.attrs) {
            return self.resolve_explicit_path(parent_path, &explicit);
        }
        self.resolve_by_convention(parent_path, &mod_item.ident.to_string())
    }

    /// `#[path = "custom.rs"]` is resolved relative to the directory
    /// containing the parent file, matching rustc's own semantics.
    /// Operation: path arithmetic + existence check, no own calls.
    fn resolve_explicit_path(&self, parent_path: &str, relative: &str) -> Option<String> {
        let parent_dir = Path::new(parent_path)
            .parent()
            .unwrap_or(Path::new(""))
            .to_path_buf();
        let candidate = parent_dir
            .join(relative)
            .to_string_lossy()
            .replace('\\', "/");
        self.known_paths
            .contains(candidate.as_str())
            .then_some(candidate)
    }

    /// Naming-convention resolution: try `{dir}/{name}.rs` then
    /// `{dir}/{name}/mod.rs` under the parent file's module directory.
    /// Operation: path arithmetic + existence checks, no own calls.
    fn resolve_by_convention(&self, parent_path: &str, mod_name: &str) -> Option<String> {
        let parent = Path::new(parent_path);
        let child_dir = if parent
            .file_stem()
            .is_some_and(|s| s == "mod" || s == "lib" || s == "main")
        {
            parent.parent().unwrap_or(Path::new("")).to_path_buf()
        } else {
            parent.with_extension("")
        };
        let file_raw = child_dir.join(format!("{mod_name}.rs"));
        let dir_raw = child_dir.join(mod_name).join("mod.rs");
        let file_lossy = file_raw.to_string_lossy();
        let dir_lossy = dir_raw.to_string_lossy();
        let candidate_file = normalize_sep(file_lossy.as_ref());
        let candidate_dir = normalize_sep(dir_lossy.as_ref());
        if self.known_paths.contains(candidate_file.as_ref()) {
            Some(candidate_file.into_owned())
        } else if self.known_paths.contains(candidate_dir.as_ref()) {
            Some(candidate_dir.into_owned())
        } else {
            None
        }
    }
}

/// Convert OS-native path separators into the forward-slash form used
/// by `known_paths`. Returns `Cow::Borrowed` on Unix and on Windows
/// paths without backslashes; allocates only when a replacement is
/// actually needed.
fn normalize_sep(path: &str) -> Cow<'_, str> {
    if cfg!(windows) && path.contains('\\') {
        Cow::Owned(path.replace('\\', "/"))
    } else {
        Cow::Borrowed(path)
    }
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
