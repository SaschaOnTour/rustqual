//! The contract behind "a `use` is exposure, not consumption", tested at its
//! own grain: which segments of a path are heads, and what the pre-pass reads.

use std::collections::HashSet;

use crate::adapters::analyzers::dry::use_bindings::{collect_declared_names, heads_of, leaf_head};

fn strings(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| n.to_string()).collect()
}

/// (path, declared modules) → the heads. The first segment always; the last
/// one when the qualifier can be a module — by convention, or by declaration
/// (which includes an alias of a declared module, see the pre-pass test).
const HEADS: &[(&[&str], &[&str], &[&str])] = &[
    (&["perform"], &[], &["perform"]),
    (&["facade", "perform"], &[], &["facade", "perform"]),
    (&["crate", "TopAlias"], &[], &["crate", "TopAlias"]),
    (&["super", "super", "w"], &[], &["super", "w"]),
    (&["Kind", "Circle"], &[], &["Kind"]),
    (&["std", "cmp", "Ordering", "Less"], &[], &["std"]),
    (&["Self", "helper"], &[], &["Self"]),
    (&["Type", "new"], &[], &["Type"]),
    // A declared module named like a type still carries bindings, at any depth.
    (&["Facade", "Alias"], &["Facade"], &["Facade", "Alias"]),
    (&["a", "Facade", "Alias"], &["Facade"], &["a", "Alias"]),
    (&["Type"], &[], &["Type"]),
];

#[test]
fn a_head_is_where_a_use_binding_can_be_reached() {
    for (path, modules, expected) in HEADS {
        let modules: HashSet<String> = strings(modules).into_iter().collect();
        assert_eq!(
            heads_of(&strings(path), &modules),
            strings(expected),
            "{path:?}"
        );
    }
}

/// (path, declared modules) → the leaf head. A function binding is always
/// the leaf: the first segment of a qualified path is a module or a type,
/// never a call.
const LEAF_HEADS: &[(&[&str], &[&str], Option<&str>)] = &[
    (&["perform"], &[], Some("perform")),
    (&["facade", "perform"], &[], Some("perform")),
    (&["crate", "w"], &[], Some("w")),
    (&["Type", "new"], &[], None),
    (&["Self", "helper"], &[], None),
    (&["Facade", "perform"], &["Facade"], Some("perform")),
    (&[], &[], None),
];

#[test]
fn the_leaf_head_is_the_last_segment_or_nothing() {
    for (path, modules, expected) in LEAF_HEADS {
        let modules: HashSet<String> = strings(modules).into_iter().collect();
        let path = strings(path);
        let leaf = leaf_head(&path, &modules).map(String::as_str);
        assert_eq!(leaf, *expected, "{path:?}");
    }
}

/// A workspace slice with every shape the pre-pass has to read: modules at
/// any depth, enums at any depth, and the names a `use` or `extern crate`
/// makes a module of.
const DECLARATIONS: &str = "mod plain;\n\
    mod outer { pub mod inner { pub enum Deep { A, B } } }\n\
    fn body() { mod local { pub enum Hidden { X } } }\n\
    use outer as Outer;\n\
    use Outer as O;\n\
    use Top as T;\n\
    use crate as API;\n\
    extern crate self as SelfCrate;\n\
    extern crate dep as Dep;\n\
    use other_dep as Ext;\n\
    use Camel as Cam;\n\
    use {Camel2::thing, third::x};\n\
    struct lower;\n\
    use lower as Low;\n\
    mod unrelated { pub struct Dep2; }\n\
    use Dep2 as Api2;\n\
    macro_rules! expose { () => { mod Generated { pub enum Gen { V } } }; }\n\
    pub enum Top { One }";

/// Every name the pre-pass counts as a module in `DECLARATIONS`. An alias of
/// a module is a module name, through any number of hops. The crate root and
/// an external crate are modules too: `extern crate` names one outright, and
/// whatever a `use` path starts with may be one — `other_dep`, `Camel`,
/// `Camel2`, `third` are external crates whatever their spelling, and `Dep2`,
/// `Top` or `lower` may be a crate the `use` sees rather than the same-named
/// type declared elsewhere. Bare-name grain cannot tell, so all of them
/// count, and so do their aliases. What a `macro_rules!` transcriber
/// declares (`Generated`, `Gen`) is read as if it were written out.
const MODULES: &[&str] = &[
    "API",
    "Api2",
    "Cam",
    "Camel",
    "Camel2",
    "Dep",
    "Dep2",
    "Ext",
    "Generated",
    "Low",
    "O",
    "Outer",
    "SelfCrate",
    "T",
    "Top",
    "crate",
    "dep",
    "inner",
    "local",
    "lower",
    "other_dep",
    "outer",
    "plain",
    "self",
    "super",
    "third",
    "unrelated",
];

fn declared() -> crate::adapters::analyzers::dry::use_bindings::DeclaredNames {
    let parsed = vec![(
        "src/lib.rs".to_string(),
        DECLARATIONS.to_string(),
        syn::parse_file(DECLARATIONS).unwrap(),
    )];
    collect_declared_names(&parsed)
}

#[test]
fn the_pre_pass_reads_every_module_wherever_it_sits() {
    let declared = declared();
    let mut modules: Vec<&str> = declared.modules.iter().map(String::as_str).collect();
    modules.sort();
    assert_eq!(modules, MODULES);
}

#[test]
fn the_pre_pass_reads_every_enum_wherever_it_sits() {
    let declared = declared();
    assert_eq!(declared.enums["Deep"], strings(&["A", "B"]));
    assert_eq!(declared.enums["Hidden"], strings(&["X"]));
    assert_eq!(declared.enums["Top"], strings(&["One"]));
    assert_eq!(declared.enums["Gen"], strings(&["V"]));
}
