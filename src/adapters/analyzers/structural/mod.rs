pub(crate) mod btc;
pub(crate) mod deh;
mod identity;
pub(crate) mod iet;
pub(crate) mod nms;
pub(crate) mod oi;
pub(crate) mod sit;
pub(crate) mod slm;

use std::collections::{HashMap, HashSet};

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

    /// The `// qual:allow(<dim>, <target>)` suppression-target name for this
    /// check (the lowercased `code`). Used by the marking pass; the orphan
    /// detector resolves the same name from the projected finding's code via
    /// [`target_name_for_code`].
    /// Operation: delegates to the shared code→target map.
    pub fn target_name(&self) -> &'static str {
        target_name_for_code(self.code()).unwrap_or("")
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

/// Map a structural rule code (`"SIT"`) to its lowercase suppression-target
/// name (`"sit"`), or `None` for an unknown code. The single source for the
/// code→target mapping, shared by `StructuralWarningKind::target_name` (marking)
/// and the orphan detector, which only has the code string from the projected
/// `Structural` finding.
/// Operation: match dispatch returning string literals.
pub fn target_name_for_code(code: &str) -> Option<&'static str> {
    match code {
        "BTC" => Some("btc"),
        "SLM" => Some("slm"),
        "NMS" => Some("nms"),
        "OI" => Some("oi"),
        "SIT" => Some("sit"),
        "DEH" => Some("deh"),
        "IET" => Some("iet"),
        _ => None,
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
    /// type_name → every file defining a type of that name (structs +
    /// enums), in walk order. All of them, not the last: two top-level types
    /// may share a name, and keeping one let the directory walk order decide
    /// which, so OI flagged a correct impl on one machine and not on another.
    pub type_defs: HashMap<String, Vec<String>>,
    /// trait_name → TraitInfo
    /// trait_name → every trait of that name, in walk order. All of them,
    /// for the reason `type_defs` keeps all: impls are counted by bare trait
    /// name, and keeping the last definition let the walk order decide which
    /// trait a single-impl finding landed on.
    pub trait_defs: HashMap<String, Vec<TraitInfo>>,
    /// trait_name → list of (impl_type, file, certain_path) — production
    /// impls only; `certain_path` when the trait is named bare, is no prelude
    /// name, and no cfg gates the impl (see `identity`).
    pub trait_impls: HashMap<String, Vec<(String, String, bool)>>,
    /// (file, name) for every name a `use` binds, from any root — `self::`
    /// and `super::` may reach a re-export of a foreign trait as well as
    /// `crate::` can. A bare `impl Name` in that file may mean it.
    pub outside_imports: HashSet<(String, String)>,
    /// Files holding a glob `use`: any bare name there may come from it.
    pub glob_files: HashSet<String>,
    /// file → its logical places, (crate root, top-level module), from the
    /// module tree walk; see `shared::reachability::file_modules`.
    pub file_modules: HashMap<String, Vec<(String, String)>>,
    /// Depth of `#[cfg(…)]`-gated inline modules around the item being
    /// collected. rustqual evaluates no cfg, so an impl in there may not
    /// exist and is never *certain* for SIT.
    pub(crate) cfg_depth: usize,
    /// trait_name → number of impls living in `#[cfg(test)]` code (a whole test
    /// file, an inline `#[cfg(test)] mod` block, or an item-level `#[cfg(test)]`
    /// on the impl). Not in `trait_impls` (metadata skips test code), but they
    /// count toward a trait's TOTAL
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
    /// Behind a `cfg` rustqual does not evaluate — on the trait, an inline
    /// module around it, its file, or the `mod` pulling the file in — so it
    /// may not exist, and SIT reports nothing about it.
    pub gated: bool,
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
        outside_imports: HashSet::new(),
        glob_files: HashSet::new(),
        file_modules: HashMap::new(),
        cfg_depth: 0,
        cfg_test_trait_impl_counts: collect_cfg_test_trait_impl_counts(parsed, cfg_test_files),
        inherent_impls: Vec::new(),
    };
    let places = crate::adapters::shared::reachability::file_places(parsed);
    meta.file_modules = places.places;
    // Gated by the module tree; a file no crate root reaches is judged by
    // its own `#![cfg]` alone.
    let gated_files: HashSet<&String> = parsed
        .iter()
        .map(|(path, _, _)| path)
        .filter(|path| places.gated.contains(*path))
        .collect();
    let file_gated = |path: &String, file: &syn::File| {
        gated_files.contains(path) || identity::has_cfg(&file.attrs)
    };
    parsed.iter().for_each(|(path, _, syntax)| {
        if cfg_test_files.contains(path) {
            return;
        }
        meta.cfg_depth = usize::from(file_gated(path, syntax));
        syntax.items.iter().for_each(|item| {
            collect_item_metadata(item, path, &mut meta);
        });
    });
    meta.cfg_depth = 0;
    meta
}

impl StructuralMetadata {
    /// Record one more file defining a type of this name.
    /// Operation: one map entry push, no own calls.
    fn define_type(&mut self, name: &str, path: &str) {
        self.type_defs
            .entry(name.to_string())
            .or_default()
            .push(path.to_string());
    }
}

/// Extract metadata from a single top-level item.
/// Operation: match dispatch on item kind, own calls hidden in closures.
fn collect_item_metadata(item: &syn::Item, path: &str, meta: &mut StructuralMetadata) {
    let cfg_test = |attrs: &[syn::Attribute]| -> bool { has_cfg_test_attr(attrs) };
    match item {
        // A `#[cfg(test)]` type does not exist in a production build, so it
        // cannot be what a production impl means; counting it let a test
        // fixture vouch for an orphaned impl of a same-named type.
        syn::Item::Enum(e) if !cfg_test(&e.attrs) => {
            let name = e.ident.to_string();
            let variants: Vec<String> = e.variants.iter().map(|v| v.ident.to_string()).collect();
            meta.define_type(&name, path);
            meta.enum_defs.insert(name, (path.to_string(), variants));
        }
        syn::Item::Struct(s) if !cfg_test(&s.attrs) => meta.define_type(&s.ident.to_string(), path),
        // A `#[cfg(test)]` trait is no second definition for SIT's
        // ambiguity rule — it does not exist where the production impls do.
        syn::Item::Trait(t) if !cfg_test(&t.attrs) => {
            let is_pub = matches!(t.vis, syn::Visibility::Public(_));
            let method_count = t
                .items
                .iter()
                .filter(|i| matches!(i, syn::TraitItem::Fn(_)))
                .count();
            meta.trait_defs
                .entry(t.ident.to_string())
                .or_default()
                .push(TraitInfo {
                    file: path.to_string(),
                    line: t.ident.span().start().line,
                    is_pub,
                    method_count,
                    gated: identity::has_cfg(&t.attrs) || meta.cfg_depth > 0,
                });
        }
        syn::Item::Impl(imp) if !cfg_test(&imp.attrs) => identity::record_impl(imp, path, meta),
        syn::Item::Use(u) if !cfg_test(&u.attrs) => identity::record_outside_imports(u, path, meta),
        syn::Item::Mod(m) if !cfg_test(&m.attrs) => {
            let gated = usize::from(identity::has_cfg(&m.attrs));
            meta.cfg_depth += gated;
            m.content.iter().for_each(|(_, items)| {
                items
                    .iter()
                    .for_each(|i| collect_item_metadata(i, path, meta));
            });
            meta.cfg_depth -= gated;
        }
        _ => {}
    }
}

/// Extract the type name from an impl block.
/// Operation: match on self_ty, no own calls.
pub(crate) fn extract_impl_type_name(imp: &syn::ItemImpl) -> Option<String> {
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
        syntax
            .items
            .iter()
            .for_each(|item| count_cfg_test_trait_impls(item, in_test, &mut counts));
    });
    counts
}

/// Recurse one item, counting `#[cfg(test)]` trait impls by last-path-segment
/// trait name, descending into submodules. "In test" = a whole test file, an
/// inline `#[cfg(test)] mod`, or an item-level `#[cfg(test)]` on the impl.
///
/// Counting is deliberately PURELY name-based: it makes no attempt to resolve
/// which trait a same-named impl actually targets. That guarantees it never
/// drops a real test double — so a legitimate DI seam is never re-flagged (no
/// false positive, the original bug class). The accepted cost is a safe,
/// documented false NEGATIVE: an unrelated test-only trait that happens to share
/// a production trait's last-segment name is counted toward it and can make SIT
/// under-report that production trait. (Earlier attempts to disambiguate by
/// lexical scope / path form re-introduced false positives — e.g. a nested
/// `super::Clock` double dropped because the nested module defined its own
/// `Clock` — because correctly resolving `self::`/`super::`/glob/re-export/
/// absolute paths IS trait identity, which the name-keyed metadata models on
/// neither the production nor the test side. The production side has the same
/// name-collision limitation.)
/// Operation: match dispatch; own calls hidden in closures for IOSP.
fn count_cfg_test_trait_impls(
    item: &syn::Item,
    in_test: bool,
    counts: &mut HashMap<String, usize>,
) {
    let cfg_test = |attrs: &[syn::Attribute]| -> bool { has_cfg_test_attr(attrs) };
    match item {
        syn::Item::Impl(imp) if in_test || cfg_test(&imp.attrs) => {
            if let Some((_, tp, _)) = &imp.trait_ {
                if let Some(name) = tp.segments.last().map(|s| s.ident.to_string()) {
                    *counts.entry(name).or_insert(0) += 1;
                }
            }
        }
        syn::Item::Mod(m) => {
            let mod_in_test = in_test || cfg_test(&m.attrs);
            m.content.iter().for_each(|(_, items)| {
                items
                    .iter()
                    .for_each(|i| count_cfg_test_trait_impls(i, mod_in_test, counts));
            });
        }
        _ => {}
    }
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
        syn::Item::Impl(imp) if !has_cfg_test_attr(&imp.attrs) => {
            if imp.trait_.is_some() {
                return;
            }
            imp.items.iter().for_each(|i| {
                if let syn::ImplItem::Fn(method) = i {
                    if !has_cfg_test_attr(&method.attrs) {
                        callback(method, path);
                    }
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
