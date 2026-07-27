//! What excuses a declaration from DRY-006: the `dead_code` lint level
//! in all its inherited forms, and the two markers — including what they
//! keep alive in turn.

use std::collections::HashSet;

use super::{detect, detect_with, markers, names, Markers};
use crate::adapters::analyzers::dry::dead_types::*;

#[test]
fn allow_dead_code_excludes_the_declaration() {
    // The author already told the compiler this is intentional.
    assert!(names("#[allow(dead_code)]\nstruct Kept;").is_empty());
}

#[test]
fn qual_api_excludes_the_declaration() {
    // The whole point of the marker: consumers live outside the analysed code,
    // so having no in-workspace user is expected. Before DRY-006 this marker
    // did nothing on a type and was reported as inert.
    let found = detect_with(
        "// qual:api\npub struct Entry;",
        &markers(&[1]),
        &Markers::new(),
        &HashSet::new(),
    );
    assert!(found.is_empty(), "qual:api must exclude a type: {found:?}");
}

#[test]
fn qual_test_helper_silences_test_only_but_not_unused() {
    // The marker's narrow purpose, mirrored from DRY-002: it excuses "only
    // tests use it", never "nothing uses it".
    let used_by_tests = "// qual:test_helper\npub struct Fixture;\n#[cfg(test)]\n\
                         mod tests { use super::Fixture; fn t() { let _ = Fixture; } }";
    assert!(
        detect_with(
            used_by_tests,
            &Markers::new(),
            &markers(&[1]),
            &HashSet::new()
        )
        .is_empty(),
        "test_helper excuses the test-only finding"
    );
    let used_by_nobody = "// qual:test_helper\npub struct Fixture;";
    assert_eq!(
        detect_with(
            used_by_nobody,
            &Markers::new(),
            &markers(&[1]),
            &HashSet::new()
        )
        .len(),
        1,
        "a helper nothing refers to is still dead"
    );
}

#[test]
fn an_underscore_prefixed_declaration_is_not_reported() {
    // `_name` is the language's own "deliberately unused" convention, and
    // rustc's dead_code lint honours it. Contradicting that would make the
    // check argue with the compiler.
    assert!(names("const _KEPT_FOR_DOCS: u8 = 1;").is_empty());
    assert!(names("struct _Marker;").is_empty());
}

#[test]
fn allow_dead_code_is_inherited_from_the_file() {
    // Rust lint levels are inherited. `#![allow(dead_code)]` at the top of a
    // file covers everything in it, and the documentation promises that
    // `#[allow(dead_code)]` exempts a declaration from DRY-006.
    assert!(names("#![allow(dead_code)]\nstruct Intentional;").is_empty());
}

#[test]
fn allow_dead_code_is_inherited_from_an_enclosing_module() {
    // The generated-code idiom: one attribute on the module, not on each item.
    assert!(names("#[allow(dead_code)]\nmod generated { struct Generated; }").is_empty());
}

#[test]
fn an_allow_on_a_sibling_module_does_not_leak() {
    // The inherited context has to be restored on the way out, or one annotated
    // module would silence the rest of the file.
    let found = detect("#[allow(dead_code)]\nmod a { struct Kept; }\nmod b { struct Dead; }");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].name, "Dead");
}

#[test]
fn allow_dead_code_is_inherited_from_an_enclosing_function() {
    // A lint level is in force for the whole lexical scope, so a local type in
    // an excused function is excused too.
    assert!(names("#[allow(dead_code)]\nfn f() { struct Local; }").is_empty());
}

#[test]
fn an_inner_deny_overrides_an_inherited_allow() {
    // Rust resolves lint levels innermost-first: the file-level allow does not
    // survive a `deny` on the declaration. Modelling inheritance as a one-way
    // flag silently kept suppressing what the author re-armed.
    let found = detect("#![allow(dead_code)]\n#[deny(dead_code)]\nstruct MustBeUsed;");
    assert_eq!(found.len(), 1, "the inner deny wins: {found:?}");
    assert_eq!(found[0].name, "MustBeUsed");
}

#[test]
fn the_last_lint_attribute_in_source_order_wins() {
    // Rust evaluates lint attributes in the order they are written; a later
    // level overrides an earlier one. Scanning a fixed severity order instead
    // reports a declaration the author explicitly allowed.
    assert!(
        names("#[deny(dead_code)]\n#[allow(dead_code)]\nstruct Intentional;").is_empty(),
        "the trailing allow wins"
    );
    assert_eq!(
        names("#[allow(dead_code)]\n#[deny(dead_code)]\nstruct Reported;"),
        vec!["Reported".to_string()],
        "the trailing deny wins"
    );
}

#[test]
fn forbid_cannot_be_downgraded_by_an_inner_allow() {
    // `forbid` is the one level a narrower scope may not relax — for rustc an
    // inner `allow` under it is an error, so honouring the allow would silence
    // something the compiler never would.
    let found = detect("#![forbid(dead_code)]\n#[allow(dead_code)]\nstruct StillReported;");
    assert_eq!(found.len(), 1, "forbid survives the inner allow: {found:?}");
}

#[test]
fn an_exempt_declaration_keeps_what_it_names_alive() {
    // `#[allow(dead_code)]` excuses the declaration itself, so what it refers to
    // has a live user. Seeding only the roots would report `Named` as dead and
    // send the author to delete a field's type.
    let found = names("#[allow(dead_code)]\nstruct Kept { n: Named }\nstruct Named;");
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn qual_api_on_the_entry_keeps_what_its_methods_name_alive() {
    // The remedy the message names has to work through a whole cluster: one
    // marker on the entry point, not one per type it reaches. The blank lines
    // keep `Ast` outside the marker's own annotation window, or the test would
    // pass for the wrong reason.
    let code = "// qual:api\npub struct Parser;\n\n\n\n\n\
                impl Parser { pub fn parse(&self) -> Ast { Ast } }\npub struct Ast;";
    let found = detect_with(code, &markers(&[1]), &Markers::new(), &HashSet::new());
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn qual_api_on_one_member_keeps_the_whole_cycle_alive() {
    // Same for a ring: marking the entry is the documented escape, and a
    // library author must not have to mark every type the entry reaches.
    let code = "// qual:api\npub struct A { b: B }\n\n\n\n\n\
                pub struct B { a: Option<Box<A>> }";
    let found = detect_with(code, &markers(&[1]), &Markers::new(), &HashSet::new());
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_test_helper_keeps_what_it_names_alive() {
    // Mirrors DRY-002, where a `qual:test_helper` function's callees count as
    // production-called because the helper itself sits in production code.
    // Without it the marker would silence one finding and create another for
    // every type the helper names — the author marks a fixture and is sent to
    // annotate its field types one by one.
    let code = "// qual:test_helper\npub struct Fixture { p: Payload }\n\n\n\n\
                pub struct Payload;\n#[cfg(test)]\n\
                mod tests { use super::Fixture; fn t(f: Fixture) { let _ = f; } }";
    let found = detect_with(code, &Markers::new(), &markers(&[1]), &HashSet::new());
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn an_ffi_export_keeps_itself_and_what_it_names_alive() {
    // `#[no_mangle]` makes an item a live root for rustc's own dead_code lint:
    // it is reachable from outside the compiled artefact by definition. Judging
    // it was already wrong; with reachability the mistake cascades to
    // everything the export names.
    let code = "pub struct Descriptor { pub name: &'static str }\nconst NAME: &str = \"plugin\";\n\
                #[no_mangle]\npub static PLUGIN: Descriptor = Descriptor { name: NAME };";
    assert!(names(code).is_empty(), "{:?}", detect(code));
}

#[test]
fn the_edition_2024_spelling_of_an_export_counts_too() {
    // Rust 2024 requires `#[unsafe(no_mangle)]`; reading only the bare form
    // would make the fix expire with the next edition bump.
    let code = "pub struct Descriptor { pub name: &'static str }\n\
                #[unsafe(no_mangle)]\npub static PLUGIN: Descriptor = Descriptor { name: \"x\" };";
    assert!(names(code).is_empty(), "{:?}", detect(code));
}

#[test]
fn an_exported_symbol_by_name_counts_too() {
    let code = "pub struct Descriptor { pub name: &'static str }\n\
                #[export_name = \"plugin_entry\"]\n\
                pub static PLUGIN: Descriptor = Descriptor { name: \"x\" };";
    assert!(names(code).is_empty(), "{:?}", detect(code));
}

/// Detect over a two-file module tree: `parent` declares `mod child;`.
fn detect_tree(parent_path: &str, parent: &str, child_path: &str, child: &str) -> Vec<String> {
    let parsed = vec![
        (
            parent_path.to_string(),
            parent.to_string(),
            syn::parse_file(parent).expect("parse parent"),
        ),
        (
            child_path.to_string(),
            child.to_string(),
            syn::parse_file(child).expect("parse child"),
        ),
    ];
    detect_dead_types(&parsed, &Markers::new(), &Markers::new(), &HashSet::new())
        .into_iter()
        .map(|w| w.name)
        .collect()
}

#[test]
fn allow_dead_code_is_inherited_across_the_file_boundary() {
    // A lint level covers everything below it, and rustc does not stop at the
    // file a module happens to live in. Reading only the declaring file's own
    // attributes reported declarations the author had excused one level up —
    // a false finding by this check's own documented rule.
    let found = detect_tree(
        "src/lib.rs",
        "pub mod inner;",
        "src/inner.rs",
        "#![allow(dead_code)]\npub struct Excused;",
    );
    assert!(found.is_empty(), "same file, the baseline: {found:?}");
    let across = detect_tree(
        "src/lib.rs",
        "#![allow(dead_code)]\npub mod inner;",
        "src/inner.rs",
        "pub struct Excused;",
    );
    assert!(
        across.is_empty(),
        "inner attribute one level up: {across:?}"
    );
}

#[test]
fn an_allow_on_the_mod_declaration_covers_the_file_it_names() {
    // The other spelling, and the more common one: the attribute sits on
    // `mod child;` in the parent, while what it excuses lives in another file.
    let found = detect_tree(
        "src/lib.rs",
        "#[allow(dead_code)]\npub mod inner;",
        "src/inner.rs",
        "pub struct Excused;",
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn an_inherited_allow_does_not_leak_to_a_sibling_module() {
    // The restore half: one excused module must not silence the rest of the
    // tree, or the inheritance would be worse than not having it.
    let parsed = vec![
        ("src/lib.rs", "#[allow(dead_code)]\npub mod a;\npub mod b;"),
        ("src/a.rs", "pub struct Excused;"),
        ("src/b.rs", "pub struct Reported;"),
    ];
    let parsed: Vec<(String, String, syn::File)> = parsed
        .into_iter()
        .map(|(p, c)| {
            (
                p.to_string(),
                c.to_string(),
                syn::parse_file(c).expect("parse"),
            )
        })
        .collect();
    let found: Vec<String> =
        detect_dead_types(&parsed, &Markers::new(), &Markers::new(), &HashSet::new())
            .into_iter()
            .map(|w| w.name)
            .collect();
    assert_eq!(found, vec!["Reported".to_string()], "{found:?}");
}

#[test]
fn a_deny_in_the_child_file_revokes_an_inherited_allow() {
    // Rust resolves levels innermost-first across files exactly as within one,
    // so the inheritance must be a level, not a one-way flag.
    let found = detect_tree(
        "src/lib.rs",
        "#![allow(dead_code)]\npub mod inner;",
        "src/inner.rs",
        "#![deny(dead_code)]\npub struct MustBeUsed;",
    );
    assert_eq!(found, vec!["MustBeUsed".to_string()], "{found:?}");
}

#[test]
fn the_inherited_allow_reaches_a_grandchild() {
    // Levels are inherited all the way down, not one hop.
    let parsed = vec![
        ("src/lib.rs", "#![allow(dead_code)]\npub mod a;"),
        ("src/a.rs", "pub mod b;"),
        ("src/a/b.rs", "pub struct Excused;"),
    ];
    let parsed: Vec<(String, String, syn::File)> = parsed
        .into_iter()
        .map(|(p, c)| {
            (
                p.to_string(),
                c.to_string(),
                syn::parse_file(c).expect("parse"),
            )
        })
        .collect();
    let found = detect_dead_types(&parsed, &Markers::new(), &Markers::new(), &HashSet::new());
    assert!(found.is_empty(), "{found:?}");
}
