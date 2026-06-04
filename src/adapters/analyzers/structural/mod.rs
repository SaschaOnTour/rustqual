// qual:allow(coupling) reason: "leaf analysis module — high instability is expected"
pub(crate) mod btc;
pub(crate) mod deh;
pub(crate) mod iet;
pub(crate) mod nms;
pub(crate) mod oi;
pub(crate) mod sit;
pub(crate) mod slm;

use std::collections::{HashMap, HashSet};
use syn::spanned::Spanned;

use crate::config::StructuralConfig;
use crate::findings::Dimension;

/// A single structural warning.
#[derive(Debug, Clone)]
pub struct StructuralWarning {
    pub file: String,
    pub line: usize,
    pub name: String,
    pub kind: StructuralWarningKind,
    pub dimension: Dimension,
    pub suppressed: bool,
}

/// The kind of structural issue detected.
#[derive(Debug, Clone)]
pub enum StructuralWarningKind {
    /// BTC: impl Trait where all methods are todo!/unimplemented!/panic!("not implemented").
    BrokenTraitContract { trait_name: String },
    /// SLM: method takes &self/&mut self but never references self.
    SelflessMethod,
    /// NMS: method takes &mut self but never writes to self.
    NeedlessMutSelf,
    /// OI: impl block in different file than type definition.
    OrphanedImpl { defining_file: String },
    /// SIT: non-pub trait with exactly 1 implementation.
    SingleImplTrait { impl_type: String },
    /// DEH: use of Any::downcast_ref/downcast_mut/downcast.
    DowncastEscapeHatch,
    /// IET: pub fns in same module return different Result error types.
    InconsistentErrorTypes { error_types: Vec<String> },
}

impl StructuralWarningKind {
    /// Return the short rule code.
    /// Operation: match dispatch returning string literals.
    pub fn code(&self) -> &'static str {
        match self {
            Self::BrokenTraitContract { .. } => "BTC",
            Self::SelflessMethod => "SLM",
            Self::NeedlessMutSelf => "NMS",
            Self::OrphanedImpl { .. } => "OI",
            Self::SingleImplTrait { .. } => "SIT",
            Self::DowncastEscapeHatch => "DEH",
            Self::InconsistentErrorTypes { .. } => "IET",
        }
    }

    /// Return the human-readable detail string.
    /// Operation: match dispatch with string formatting.
    pub fn detail(&self) -> String {
        match self {
            Self::BrokenTraitContract { trait_name } => format!("stub impl of trait {trait_name}"),
            Self::SelflessMethod => "self never referenced".to_string(),
            Self::NeedlessMutSelf => "&mut self but no mutation".to_string(),
            Self::OrphanedImpl { defining_file } => format!("type defined in {defining_file}"),
            Self::SingleImplTrait { impl_type } => format!("only impl: {impl_type}"),
            Self::DowncastEscapeHatch => "downcast usage".to_string(),
            Self::InconsistentErrorTypes { error_types } => {
                format!("error types: {}", error_types.join(", "))
            }
        }
    }
}

/// Results of structural analysis.
#[derive(Debug, Clone, Default)]
pub struct StructuralAnalysis {
    pub warnings: Vec<StructuralWarning>,
}

/// Cross-file metadata collected in a first pass for structural analysis.
pub(crate) struct StructuralMetadata {
    /// enum_name → (defining_file, variant_names)
    pub enum_defs: HashMap<String, (String, Vec<String>)>,
    /// type_name → defining_file (structs + enums)
    pub type_defs: HashMap<String, String>,
    /// trait_name → TraitInfo
    pub trait_defs: HashMap<String, TraitInfo>,
    /// trait_name → list of (impl_type, file) — production impls only
    pub trait_impls: HashMap<String, Vec<(String, String)>>,
    /// trait_name → number of impls living in `#[cfg(test)]` code (whole test
    /// files or inline `#[cfg(test)] mod` blocks). Not in `trait_impls`
    /// (metadata skips test code), but they count toward a trait's TOTAL
    /// implementor count so SIT does not flag a trait whose only extra
    /// implementers are test doubles — the idiomatic DI / test-seam pattern.
    pub cfg_test_trait_impl_counts: HashMap<String, usize>,
    /// (type_name, impl_file, impl_block_line) for inherent impls
    pub inherent_impls: Vec<(String, String, usize)>,
}

/// Metadata about a trait definition.
pub(crate) struct TraitInfo {
    pub file: String,
    pub line: usize,
    pub is_pub: bool,
    pub method_count: usize,
}

/// Collect structural metadata from all parsed files. Whole test files
/// (in `cfg_test_files`) are skipped so OI/SIT — which key off this
/// metadata — never flag types/traits/impls that live in test code;
/// inline `#[cfg(test)] mod` blocks are already skipped per-item.
/// Operation: iterates files and visits AST nodes, no own calls.
pub(crate) fn collect_metadata(
    parsed: &[(String, String, syn::File)],
    cfg_test_files: &HashSet<String>,
) -> StructuralMetadata {
    let mut meta = StructuralMetadata {
        enum_defs: HashMap::new(),
        type_defs: HashMap::new(),
        trait_defs: HashMap::new(),
        trait_impls: HashMap::new(),
        cfg_test_trait_impl_counts: collect_cfg_test_trait_impl_counts(parsed, cfg_test_files),
        inherent_impls: Vec::new(),
    };
    parsed.iter().for_each(|(path, _, syntax)| {
        if cfg_test_files.contains(path) {
            return;
        }
        syntax.items.iter().for_each(|item| {
            collect_item_metadata(item, path, &mut meta);
        });
    });
    meta
}

/// Extract metadata from a single top-level item.
/// Operation: match dispatch on item kind, own calls hidden in closures.
fn collect_item_metadata(item: &syn::Item, path: &str, meta: &mut StructuralMetadata) {
    let impl_type_name = |imp: &syn::ItemImpl| -> Option<String> { extract_impl_type_name(imp) };
    let cfg_test = |attrs: &[syn::Attribute]| -> bool { has_cfg_test_attr(attrs) };
    match item {
        syn::Item::Enum(e) => {
            let name = e.ident.to_string();
            let variants: Vec<String> = e.variants.iter().map(|v| v.ident.to_string()).collect();
            meta.type_defs.insert(name.clone(), path.to_string());
            meta.enum_defs.insert(name, (path.to_string(), variants));
        }
        syn::Item::Struct(s) => {
            meta.type_defs.insert(s.ident.to_string(), path.to_string());
        }
        syn::Item::Trait(t) => {
            let is_pub = matches!(t.vis, syn::Visibility::Public(_));
            let method_count = t
                .items
                .iter()
                .filter(|i| matches!(i, syn::TraitItem::Fn(_)))
                .count();
            meta.trait_defs.insert(
                t.ident.to_string(),
                TraitInfo {
                    file: path.to_string(),
                    line: t.ident.span().start().line,
                    is_pub,
                    method_count,
                },
            );
        }
        syn::Item::Impl(imp) if !cfg_test(&imp.attrs) => {
            if let Some(ref type_name) = impl_type_name(imp) {
                let line = imp.span().start().line;
                if let Some((_, ref tp, _)) = imp.trait_ {
                    let tn = tp
                        .segments
                        .last()
                        .map(|s| s.ident.to_string())
                        .unwrap_or_default();
                    meta.trait_impls
                        .entry(tn)
                        .or_default()
                        .push((type_name.clone(), path.to_string()));
                } else {
                    meta.inherent_impls
                        .push((type_name.clone(), path.to_string(), line));
                }
            }
        }
        syn::Item::Mod(m) if !cfg_test(&m.attrs) => {
            m.content.iter().for_each(|(_, items)| {
                items
                    .iter()
                    .for_each(|i| collect_item_metadata(i, path, meta));
            });
        }
        _ => {}
    }
}

/// Extract the type name from an impl block.
/// Operation: match on self_ty, no own calls.
fn extract_impl_type_name(imp: &syn::ItemImpl) -> Option<String> {
    match &*imp.self_ty {
        syn::Type::Path(tp) => tp.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

use crate::adapters::shared::cfg_test::has_cfg_test as has_cfg_test_attr;

/// Count trait impls living in `#[cfg(test)]` code, keyed by trait name. The
/// main metadata walk skips test code, so these production-invisible impls
/// would otherwise vanish from a trait's implementor count — making a test
/// seam (one prod impl + test doubles) look like a single-impl over-abstraction
/// to SIT. "Test" means a whole `#![cfg(test)]` / integration-test file, an
/// inline `#[cfg(test)] mod`, or an item-level `#[cfg(test)]` on the impl.
/// Operation: iterates files, delegates per-scope collection via a closure.
fn collect_cfg_test_trait_impl_counts(
    parsed: &[(String, String, syn::File)],
    cfg_test_files: &HashSet<String>,
) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    parsed.iter().for_each(|(path, _, syntax)| {
        let in_test = cfg_test_files.contains(path);
        collect_test_impls_in_scope(&syntax.items, in_test, &mut counts);
    });
    counts
}

/// Tally `#[cfg(test)]` trait impls in one item scope (file or module body),
/// keyed by trait name, recursing into submodules. A test impl whose trait is a
/// **single bare segment** naming a trait DEFINED in test code in the *same*
/// scope is attributed to that local test-only trait — not a same-named
/// production trait — so it is not counted; this keeps an unrelated test-only
/// `trait Clock` from suppressing the production `Clock`, while a real double in
/// a *different* test module (or via a qualified `super::Clock` path) still
/// counts. NOTE: still name-based — a test impl of an *imported/external* trait
/// that merely shares a production trait's last-segment name collides; fully
/// resolving that needs real trait identity, which the name-keyed metadata
/// models on neither the production nor the test side.
/// Operation: builds the local test-trait set, then tallies / recurses — own
/// calls hidden in closures for IOSP.
fn collect_test_impls_in_scope(
    items: &[syn::Item],
    in_test: bool,
    counts: &mut HashMap<String, usize>,
) {
    let cfg_test = |attrs: &[syn::Attribute]| -> bool { has_cfg_test_attr(attrs) };
    let local_test_traits: HashSet<String> = items
        .iter()
        .filter_map(|it| match it {
            syn::Item::Trait(t) if in_test || cfg_test(&t.attrs) => Some(t.ident.to_string()),
            _ => None,
        })
        .collect();
    items.iter().for_each(|item| match item {
        syn::Item::Impl(imp) if in_test || cfg_test(&imp.attrs) => {
            if let Some((_, tp, _)) = &imp.trait_ {
                let bare_local = tp.segments.len() == 1
                    && tp
                        .segments
                        .last()
                        .is_some_and(|s| local_test_traits.contains(&s.ident.to_string()));
                if !bare_local {
                    if let Some(name) = tp.segments.last().map(|s| s.ident.to_string()) {
                        *counts.entry(name).or_insert(0) += 1;
                    }
                }
            }
        }
        syn::Item::Mod(m) => {
            let mod_in_test = in_test || cfg_test(&m.attrs);
            if let Some((_, sub_items)) = &m.content {
                collect_test_impls_in_scope(sub_items, mod_in_test, counts);
            }
        }
        _ => {}
    });
}

/// Visit all inherent (non-trait) impl methods in parsed files, excluding test modules.
/// Operation: iterates items, dispatches to callback via closure.
pub(crate) fn visit_inherent_methods(
    parsed: &[(String, String, syn::File)],
    mut callback: impl FnMut(&syn::ImplItemFn, &str),
) {
    let visit_item = |item: &syn::Item, path: &str, cb: &mut dyn FnMut(&syn::ImplItemFn, &str)| {
        visit_item_methods(item, path, cb);
    };
    parsed.iter().for_each(|(path, _, syntax)| {
        syntax
            .items
            .iter()
            .for_each(|item| visit_item(item, path, &mut callback));
    });
}

/// Recursively visit inherent impl methods in a single item.
/// Operation: match dispatch + recursion into modules.
fn visit_item_methods(
    item: &syn::Item,
    path: &str,
    callback: &mut dyn FnMut(&syn::ImplItemFn, &str),
) {
    match item {
        syn::Item::Impl(imp) => {
            if imp.trait_.is_some() {
                return;
            }
            imp.items.iter().for_each(|i| {
                if let syn::ImplItem::Fn(method) = i {
                    callback(method, path);
                }
            });
        }
        syn::Item::Mod(m) if !has_cfg_test_attr(&m.attrs) => {
            m.content.iter().for_each(|(_, items)| {
                items
                    .iter()
                    .for_each(|i| visit_item_methods(i, path, callback));
            });
        }
        _ => {}
    }
}

/// Analyze structural issues across all parsed files.
/// Integration: orchestrates metadata collection + all 7 detectors, no logic.
pub(crate) fn analyze_structural(
    parsed: &[(String, String, syn::File)],
    config: &StructuralConfig,
) -> StructuralAnalysis {
    let cfg_test_files =
        crate::adapters::shared::cfg_test_files::collect_cfg_test_file_paths(parsed);
    let meta = collect_metadata(parsed, &cfg_test_files);

    let mut warnings = Vec::new();
    btc::detect_btc(&mut warnings, parsed, config, &cfg_test_files);
    slm::detect_slm(&mut warnings, parsed, config, &cfg_test_files);
    nms::detect_nms(&mut warnings, parsed, config, &cfg_test_files);
    deh::detect_deh(&mut warnings, parsed, config, &cfg_test_files);
    oi::detect_oi(&mut warnings, &meta, config);
    sit::detect_sit(&mut warnings, &meta, config);
    iet::detect_iet(&mut warnings, parsed, config, &cfg_test_files);

    StructuralAnalysis { warnings }
}

#[cfg(test)]
mod tests;
