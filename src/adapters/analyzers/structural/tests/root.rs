use crate::adapters::analyzers::structural::*;

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
