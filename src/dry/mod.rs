pub mod boilerplate;
pub(crate) mod call_targets;
pub mod dead_code;
pub mod fragments;
pub mod functions;
pub mod match_patterns;
pub mod wildcards;

pub use boilerplate::BoilerplateFind;
pub use dead_code::{DeadCodeKind, DeadCodeWarning};
pub use fragments::FragmentGroup;
pub use functions::{DuplicateGroup, DuplicateKind};

use syn::spanned::Spanned;
use syn::visit::Visit;

use crate::normalize::NormalizedToken;

// ── Shared visitor infrastructure ──────────────────────────────

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

// ── Shared types ────────────────────────────────────────────────

/// A function with its normalized hash information, ready for duplicate detection.
pub struct FunctionHashEntry {
    pub name: String,
    pub qualified_name: String,
    pub file: String,
    pub line: usize,
    pub hash: u64,
    pub token_count: usize,
    pub tokens: Vec<NormalizedToken>,
}

/// A declared function with metadata for dead code analysis.
pub struct DeclaredFunction {
    pub name: String,
    pub qualified_name: String,
    pub file: String,
    pub line: usize,
    pub is_test: bool,
    pub is_main: bool,
    pub is_trait_impl: bool,
    pub has_allow_dead_code: bool,
    /// Whether this function is marked as public API via `// qual:api`.
    pub is_api: bool,
}

// ── Function hash collection ────────────────────────────────────

/// Collect function hashes from all parsed files.
/// Trivial: creates visitor and delegates to visit_all_files.
pub(crate) fn collect_function_hashes(
    parsed: &[(String, String, syn::File)],
    config: &crate::config::sections::DuplicatesConfig,
) -> Vec<FunctionHashEntry> {
    let mut collector = FunctionCollector {
        config,
        file: String::new(),
        entries: Vec::new(),
        in_test: false,
        parent_type: None,
        is_trait_impl: false,
    };
    visit_all_files(parsed, &mut collector);
    collector.entries
}

/// Collect declared function metadata from all parsed files.
/// Trivial: creates visitor and delegates to visit_all_files.
pub(crate) fn collect_declared_functions(
    parsed: &[(String, String, syn::File)],
) -> Vec<DeclaredFunction> {
    let mut collector = DeclaredFnCollector {
        file: String::new(),
        functions: Vec::new(),
        in_test: false,
        parent_type: None,
        is_trait_impl: false,
    };
    visit_all_files(parsed, &mut collector);
    collector.functions
}

// ── Attribute helpers ───────────────────────────────────────────

/// Check if attributes contain `#[cfg(test)]`.
/// Operation: attribute inspection logic.
pub(crate) fn has_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Ident>()
                .is_ok_and(|ident| ident == "test")
    })
}

/// Check if attributes contain `#[test]`.
/// Operation: attribute inspection logic.
pub(crate) fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("test"))
}

/// Check if attributes contain `#[allow(dead_code)]`.
/// Operation: attribute inspection logic.
fn has_allow_dead_code(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("allow")
            && attr
                .parse_args::<syn::Ident>()
                .is_ok_and(|ident| ident == "dead_code")
    })
}

/// Build qualified name from optional parent type and base name.
/// Operation: simple string formatting, no own calls.
fn qualify_name(parent: &Option<String>, name: &str) -> String {
    parent
        .as_ref()
        .map_or_else(|| name.to_string(), |p| [p.as_str(), "::", name].concat())
}

// ── FunctionCollector (for DRY hashing) ─────────────────────────

/// AST visitor that collects function bodies and computes their normalized hashes.
struct FunctionCollector<'a> {
    config: &'a crate::config::sections::DuplicatesConfig,
    file: String,
    entries: Vec<FunctionHashEntry>,
    in_test: bool,
    parent_type: Option<String>,
    is_trait_impl: bool,
}

impl FileVisitor for FunctionCollector<'_> {
    fn reset_for_file(&mut self, file_path: &str) {
        self.file = file_path.to_string();
        self.in_test = false;
        self.parent_type = None;
        self.is_trait_impl = false;
    }
}

impl FunctionCollector<'_> {
    /// Build a hash entry for a function body, applying config filters.
    /// Operation: config checks + normalize/hash calls in closure (lenient).
    fn build_hash_entry(
        &self,
        name: &str,
        line: usize,
        body: &syn::Block,
        is_test_fn: bool,
        is_trait_impl: bool,
    ) -> Option<FunctionHashEntry> {
        let is_test = self.in_test || is_test_fn;
        if self.config.ignore_tests && is_test {
            return None;
        }
        if self.config.ignore_trait_impls && is_trait_impl {
            return None;
        }

        // Closure hides own calls to normalize_body/structural_hash (lenient mode).
        let compute = |b: &syn::Block| {
            let tokens = crate::normalize::normalize_body(b);
            let hash = crate::normalize::structural_hash(&tokens);
            (tokens, hash)
        };
        let (tokens, hash) = compute(body);

        if tokens.len() < self.config.min_tokens {
            return None;
        }

        let span = body.span();
        let line_count = span.end().line.saturating_sub(span.start().line) + 1;
        if line_count < self.config.min_lines {
            return None;
        }

        let qualify = |parent: &Option<String>, n: &str| qualify_name(parent, n);
        let qualified_name = qualify(&self.parent_type, name);

        Some(FunctionHashEntry {
            name: name.to_string(),
            qualified_name,
            file: self.file.clone(),
            line,
            hash,
            token_count: tokens.len(),
            tokens,
        })
    }
}

impl<'ast> Visit<'ast> for FunctionCollector<'_> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let name = node.sig.ident.to_string();
        let line = node.sig.ident.span().start().line;
        let is_test = has_test_attr(&node.attrs);
        if let Some(entry) = self.build_hash_entry(&name, line, &node.block, is_test, false) {
            self.entries.push(entry);
        }
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let prev_parent = self.parent_type.take();
        let prev_is_trait = self.is_trait_impl;

        self.is_trait_impl = node.trait_.is_some();
        if let syn::Type::Path(tp) = &*node.self_ty {
            if let Some(seg) = tp.path.segments.last() {
                self.parent_type = Some(seg.ident.to_string());
            }
        }

        syn::visit::visit_item_impl(self, node);

        self.parent_type = prev_parent;
        self.is_trait_impl = prev_is_trait;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let name = node.sig.ident.to_string();
        let line = node.sig.ident.span().start().line;
        let is_test = has_test_attr(&node.attrs);
        if let Some(entry) =
            self.build_hash_entry(&name, line, &node.block, is_test, self.is_trait_impl)
        {
            self.entries.push(entry);
        }
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        if let Some(ref block) = node.default {
            let name = node.sig.ident.to_string();
            let line = node.sig.ident.span().start().line;
            if let Some(entry) = self.build_hash_entry(&name, line, block, false, true) {
                self.entries.push(entry);
            }
        }
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let prev_in_test = self.in_test;
        if has_cfg_test(&node.attrs) {
            self.in_test = true;
        }
        syn::visit::visit_item_mod(self, node);
        self.in_test = prev_in_test;
    }
}

// ── DeclaredFnCollector (for dead code) ─────────────────────────

/// AST visitor that collects all declared function/method names with metadata.
struct DeclaredFnCollector {
    file: String,
    functions: Vec<DeclaredFunction>,
    in_test: bool,
    parent_type: Option<String>,
    is_trait_impl: bool,
}

impl FileVisitor for DeclaredFnCollector {
    fn reset_for_file(&mut self, file_path: &str) {
        self.file = file_path.to_string();
        self.in_test = false;
        self.parent_type = None;
        self.is_trait_impl = false;
    }
}

impl<'ast> Visit<'ast> for DeclaredFnCollector {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let name = node.sig.ident.to_string();
        let line = node.sig.ident.span().start().line;
        self.functions.push(DeclaredFunction {
            qualified_name: qualify_name(&self.parent_type, &name),
            is_main: name == "main",
            is_test: self.in_test || has_test_attr(&node.attrs) || has_cfg_test(&node.attrs),
            is_trait_impl: false,
            has_allow_dead_code: has_allow_dead_code(&node.attrs),
            is_api: false,
            name,
            file: self.file.clone(),
            line,
        });
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let prev_parent = self.parent_type.take();
        let prev_is_trait = self.is_trait_impl;

        self.is_trait_impl = node.trait_.is_some();
        if let syn::Type::Path(tp) = &*node.self_ty {
            if let Some(seg) = tp.path.segments.last() {
                self.parent_type = Some(seg.ident.to_string());
            }
        }

        syn::visit::visit_item_impl(self, node);

        self.parent_type = prev_parent;
        self.is_trait_impl = prev_is_trait;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let name = node.sig.ident.to_string();
        let line = node.sig.ident.span().start().line;
        self.functions.push(DeclaredFunction {
            qualified_name: qualify_name(&self.parent_type, &name),
            is_main: false,
            is_test: self.in_test || has_test_attr(&node.attrs) || has_cfg_test(&node.attrs),
            is_trait_impl: self.is_trait_impl,
            has_allow_dead_code: has_allow_dead_code(&node.attrs),
            is_api: false,
            name,
            file: self.file.clone(),
            line,
        });
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        if node.default.is_some() {
            let name = node.sig.ident.to_string();
            let line = node.sig.ident.span().start().line;
            self.functions.push(DeclaredFunction {
                qualified_name: qualify_name(&self.parent_type, &name),
                is_main: false,
                is_test: self.in_test,
                is_trait_impl: true,
                has_allow_dead_code: false,
                is_api: false,
                name,
                file: self.file.clone(),
                line,
            });
        }
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let prev_in_test = self.in_test;
        if has_cfg_test(&node.attrs) {
            self.in_test = true;
        }
        syn::visit::visit_item_mod(self, node);
        self.in_test = prev_in_test;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::sections::DuplicatesConfig;

    fn parse(code: &str) -> Vec<(String, String, syn::File)> {
        let syntax = syn::parse_file(code).expect("parse failed");
        vec![("test.rs".to_string(), code.to_string(), syntax)]
    }

    #[test]
    fn test_collect_function_hashes_empty() {
        let parsed = parse("");
        let config = DuplicatesConfig::default();
        let entries = collect_function_hashes(&parsed, &config);
        assert!(entries.is_empty());
    }

    #[test]
    fn test_collect_function_hashes_small_function_excluded() {
        // A tiny function should be excluded by min_tokens
        let parsed = parse("fn tiny() { let x = 1; }");
        let config = DuplicatesConfig::default(); // min_tokens = 30
        let entries = collect_function_hashes(&parsed, &config);
        assert!(entries.is_empty(), "Small function should be filtered out");
    }

    #[test]
    fn test_collect_function_hashes_large_function_included() {
        // A larger function with many tokens
        let code = r#"
            fn big_fn() {
                let a = 1;
                let b = 2;
                let c = a + b;
                let d = c * a;
                let e = d - b;
                let f = e + c;
                let g = f * d;
                let h = g - e;
                let i = h + f;
                let j = i * g;
            }
        "#;
        let parsed = parse(code);
        let config = DuplicatesConfig {
            min_tokens: 5, // Lower threshold for test
            min_lines: 1,
            ..DuplicatesConfig::default()
        };
        let entries = collect_function_hashes(&parsed, &config);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "big_fn");
    }

    #[test]
    fn test_collect_function_hashes_test_excluded() {
        let code = r#"
            #[cfg(test)]
            mod tests {
                fn helper() {
                    let a = 1; let b = 2; let c = a + b;
                    let d = c * a; let e = d - b; let f = e + c;
                }
            }
        "#;
        let parsed = parse(code);
        let config = DuplicatesConfig {
            min_tokens: 5,
            min_lines: 1,
            ignore_tests: true,
            ..DuplicatesConfig::default()
        };
        let entries = collect_function_hashes(&parsed, &config);
        assert!(entries.is_empty(), "Test functions should be excluded");
    }

    #[test]
    fn test_collect_function_hashes_test_included_when_not_ignored() {
        let code = r#"
            #[cfg(test)]
            mod tests {
                fn helper() {
                    let a = 1; let b = 2; let c = a + b;
                    let d = c * a; let e = d - b; let f = e + c;
                }
            }
        "#;
        let parsed = parse(code);
        let config = DuplicatesConfig {
            min_tokens: 5,
            min_lines: 1,
            ignore_tests: false,
            ..DuplicatesConfig::default()
        };
        let entries = collect_function_hashes(&parsed, &config);
        assert_eq!(entries.len(), 1, "Test functions should be included");
    }

    #[test]
    fn test_collect_function_hashes_impl_method() {
        let code = r#"
            struct Foo;
            impl Foo {
                fn method(&self) {
                    let a = 1; let b = 2; let c = a + b;
                    let d = c * a; let e = d - b; let f = e + c;
                }
            }
        "#;
        let parsed = parse(code);
        let config = DuplicatesConfig {
            min_tokens: 5,
            min_lines: 1,
            ..DuplicatesConfig::default()
        };
        let entries = collect_function_hashes(&parsed, &config);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].qualified_name, "Foo::method");
    }

    #[test]
    fn test_collect_function_hashes_trait_impl_excluded() {
        let code = r#"
            trait Bar { fn do_thing(&self); }
            struct Foo;
            impl Bar for Foo {
                fn do_thing(&self) {
                    let a = 1; let b = 2; let c = a + b;
                    let d = c * a; let e = d - b; let f = e + c;
                }
            }
        "#;
        let parsed = parse(code);
        let config = DuplicatesConfig {
            min_tokens: 5,
            min_lines: 1,
            ignore_trait_impls: true,
            ..DuplicatesConfig::default()
        };
        let entries = collect_function_hashes(&parsed, &config);
        assert!(entries.is_empty(), "Trait impl methods should be excluded");
    }

    #[test]
    fn test_has_cfg_test_positive() {
        let code = "#[cfg(test)] mod tests {}";
        let file = syn::parse_file(code).unwrap();
        if let syn::Item::Mod(m) = &file.items[0] {
            assert!(has_cfg_test(&m.attrs));
        }
    }

    #[test]
    fn test_has_cfg_test_negative() {
        let code = "#[cfg(feature = \"foo\")] mod feature_mod {}";
        let file = syn::parse_file(code).unwrap();
        if let syn::Item::Mod(m) = &file.items[0] {
            assert!(!has_cfg_test(&m.attrs));
        }
    }

    #[test]
    fn test_has_test_attr() {
        let code = "#[test] fn test_something() {}";
        let file = syn::parse_file(code).unwrap();
        if let syn::Item::Fn(f) = &file.items[0] {
            assert!(has_test_attr(&f.attrs));
        }
    }

    #[test]
    fn test_collect_declared_functions_basic() {
        let code = "fn foo() {} fn bar() {} fn main() {}";
        let parsed = parse(code);
        let declared = collect_declared_functions(&parsed);
        assert_eq!(declared.len(), 3);
        assert!(declared.iter().any(|d| d.name == "main" && d.is_main));
        assert!(declared.iter().any(|d| d.name == "foo" && !d.is_main));
    }

    #[test]
    fn test_collect_declared_functions_test_context() {
        let code = r#"
            fn production() {}
            #[cfg(test)]
            mod tests {
                fn helper() {}
                #[test]
                fn test_something() {}
            }
        "#;
        let parsed = parse(code);
        let declared = collect_declared_functions(&parsed);
        let prod = declared.iter().find(|d| d.name == "production").unwrap();
        assert!(!prod.is_test);
        let helper = declared.iter().find(|d| d.name == "helper").unwrap();
        assert!(helper.is_test);
        let test_fn = declared
            .iter()
            .find(|d| d.name == "test_something")
            .unwrap();
        assert!(test_fn.is_test);
    }

    #[test]
    fn test_collect_declared_functions_trait_impl() {
        let code = r#"
            trait Foo { fn bar(&self); }
            struct S;
            impl Foo for S {
                fn bar(&self) {}
            }
        "#;
        let parsed = parse(code);
        let declared = collect_declared_functions(&parsed);
        let bar = declared.iter().find(|d| d.name == "bar").unwrap();
        assert!(bar.is_trait_impl);
    }

    #[test]
    fn test_collect_declared_functions_allow_dead_code() {
        let code = "#[allow(dead_code)] fn unused() {}";
        let parsed = parse(code);
        let declared = collect_declared_functions(&parsed);
        assert_eq!(declared.len(), 1);
        assert!(declared[0].has_allow_dead_code);
    }
}
