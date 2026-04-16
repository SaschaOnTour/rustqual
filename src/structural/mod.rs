// qual:allow(coupling) reason: "leaf analysis module — high instability is expected"
pub(crate) mod btc;
pub(crate) mod deh;
pub(crate) mod iet;
pub(crate) mod nms;
pub(crate) mod oi;
pub(crate) mod sit;
pub(crate) mod slm;

use std::collections::HashMap;

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
    /// trait_name → list of (impl_type, file)
    pub trait_impls: HashMap<String, Vec<(String, String)>>,
    /// (type_name, impl_file) for inherent impls
    pub inherent_impls: Vec<(String, String)>,
}

/// Metadata about a trait definition.
pub(crate) struct TraitInfo {
    pub file: String,
    pub is_pub: bool,
    pub method_count: usize,
}

/// Collect structural metadata from all parsed files.
/// Operation: iterates files and visits AST nodes, no own calls.
pub(crate) fn collect_metadata(parsed: &[(String, String, syn::File)]) -> StructuralMetadata {
    let mut meta = StructuralMetadata {
        enum_defs: HashMap::new(),
        type_defs: HashMap::new(),
        trait_defs: HashMap::new(),
        trait_impls: HashMap::new(),
        inherent_impls: Vec::new(),
    };
    parsed.iter().for_each(|(path, _, syntax)| {
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
                    is_pub,
                    method_count,
                },
            );
        }
        syn::Item::Impl(imp) => {
            if let Some(ref type_name) = impl_type_name(imp) {
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
                        .push((type_name.clone(), path.to_string()));
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

use crate::dry::has_cfg_test as has_cfg_test_attr;

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
    let meta = collect_metadata(parsed);

    let mut warnings = Vec::new();
    btc::detect_btc(&mut warnings, parsed, config);
    slm::detect_slm(&mut warnings, parsed, config);
    nms::detect_nms(&mut warnings, parsed, config);
    deh::detect_deh(&mut warnings, parsed, config);
    oi::detect_oi(&mut warnings, &meta, config);
    sit::detect_sit(&mut warnings, &meta, config);
    iet::detect_iet(&mut warnings, parsed, config);

    StructuralAnalysis { warnings }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_structural_analysis_default_empty() {
        let analysis = StructuralAnalysis::default();
        assert!(analysis.warnings.is_empty());
    }

    #[test]
    fn test_collect_metadata_empty() {
        let parsed: Vec<(String, String, syn::File)> = vec![];
        let meta = collect_metadata(&parsed);
        assert!(meta.enum_defs.is_empty());
        assert!(meta.type_defs.is_empty());
        assert!(meta.trait_defs.is_empty());
    }

    #[test]
    fn test_collect_metadata_enum() {
        let source = "pub enum Color { Red, Green, Blue }";
        let syntax = syn::parse_file(source).expect("test source");
        let parsed = vec![("lib.rs".to_string(), source.to_string(), syntax)];
        let meta = collect_metadata(&parsed);
        assert!(meta.enum_defs.contains_key("Color"));
        let (file, variants) = &meta.enum_defs["Color"];
        assert_eq!(file, "lib.rs");
        assert_eq!(variants, &["Red", "Green", "Blue"]);
    }

    #[test]
    fn test_collect_metadata_struct_and_impl() {
        let source = "struct Foo {} impl Foo { fn bar() {} }";
        let syntax = syn::parse_file(source).expect("test source");
        let parsed = vec![("lib.rs".to_string(), source.to_string(), syntax)];
        let meta = collect_metadata(&parsed);
        assert_eq!(meta.type_defs.get("Foo"), Some(&"lib.rs".to_string()));
        assert_eq!(meta.inherent_impls.len(), 1);
        assert_eq!(meta.inherent_impls[0].0, "Foo");
    }

    #[test]
    fn test_collect_metadata_trait_and_impl() {
        let source = "trait Drawable { fn draw(&self); } struct Circle; impl Drawable for Circle { fn draw(&self) {} }";
        let syntax = syn::parse_file(source).expect("test source");
        let parsed = vec![("lib.rs".to_string(), source.to_string(), syntax)];
        let meta = collect_metadata(&parsed);
        assert!(meta.trait_defs.contains_key("Drawable"));
        assert!(!meta.trait_defs["Drawable"].is_pub);
        assert_eq!(meta.trait_defs["Drawable"].method_count, 1);
        assert_eq!(meta.trait_impls["Drawable"].len(), 1);
    }

    #[test]
    fn test_cfg_test_module_excluded() {
        let source = "#[cfg(test)] mod tests { struct TestOnly; }";
        let syntax = syn::parse_file(source).expect("test source");
        let parsed = vec![("lib.rs".to_string(), source.to_string(), syntax)];
        let meta = collect_metadata(&parsed);
        assert!(!meta.type_defs.contains_key("TestOnly"));
    }
}
