//! What a `use` binds, and what consuming that binding implies.
//!
//! A `use` is exposure, not consumption: it records no reference. It *binds*
//! a name in the context it sits in, and the binding is consumed where the
//! name is a **head** — the first segment of a path, or the last segment of a
//! path qualified by a module. Consuming the binding consumes what stands
//! behind it: the original of a rename, the enum of an imported variant. This
//! module holds that contract in one place — the pre-pass that reads what the
//! workspace declares, the head rule, the implications a `use` leaves behind,
//! and the fixpoint that resolves them — so the two collectors (`call_targets`
//! for functions, `type_references` for types) cannot drift apart on it.
//!
//! Everything here works at bare-name grain and errs toward *alive*: an
//! implication can only add a name to a use set, never remove one.

use std::collections::{HashMap, HashSet};

use syn::visit::Visit;

use super::split_names::ContextRefs;
use crate::adapters::shared::macro_params;
use crate::adapters::shared::use_tree::{leaves, UseLeaves};

/// Enum name → its variants, for every enum the walk declared. Pooled by bare
/// name like everything else here.
pub(crate) type EnumVariants = HashMap<String, Vec<String>>;

/// What the workspace declares that the head rule and the implications need
/// before the walk: the enums with their variants, and the module names.
#[derive(Debug, Default)]
pub(crate) struct DeclaredNames {
    pub(crate) enums: EnumVariants,
    /// Every name known to be a module: a `mod` ident, an `extern crate`
    /// ident, the path roots `crate`/`self`/`super`, whatever a `use` path
    /// starts with (`use Dep as API` names an external crate whatever its
    /// spelling — and if a `struct Dep` is declared in some other module,
    /// bare-name grain cannot tell which one the `use` sees, so by the
    /// contract the ambiguity goes to *module*, the alive direction), plus
    /// every alias a `use … as` or `extern crate … as` gives one of those,
    /// to a fixpoint. Spelling decides only for a name nothing here knows
    /// (see `heads_of`). The price is a known limit, pinned: `use lower as
    /// API; API::perform()` on a `struct lower` reads as a module binding and
    /// consumes an unrelated rename — a missed finding, never an invented
    /// one, since TQ-003 and the marker check do not read the widened set.
    pub(crate) modules: HashSet<String>,
}

/// The path roots: modules by definition, so an alias of one is a module too.
const PATH_ROOTS: [&str; 3] = ["crate", "self", "super"];

/// One pass over every file for `DeclaredNames`, wherever the items sit — a
/// nested module, a function body.
/// Integration: one visit over the files, then the result.
pub(crate) fn collect_declared_names(parsed: &[(String, String, syn::File)]) -> DeclaredNames {
    let mut collector = DeclaredNamesCollector::default();
    collector
        .names
        .modules
        .extend(PATH_ROOTS.iter().map(|r| r.to_string()));
    parsed
        .iter()
        .for_each(|(_, _, file)| collector.visit_file(file));
    // A `use` path starts at a module, an external crate, or a type in scope
    // — and the last of these cannot be told from a same-named crate here, so
    // every root counts as a module. Aliases resolve below, not here.
    let aliases: HashSet<&String> = collector.renames.iter().map(|(a, _)| a).collect();
    let roots: Vec<String> = collector
        .use_roots
        .iter()
        .filter(|root| !aliases.contains(root))
        .cloned()
        .collect();
    collector.names.modules.extend(roots);
    // "If the original is a module, so is the alias" — the pairs reversed,
    // to a fixpoint, so an alias of an alias resolves too.
    let aliased: Vec<(String, String)> = collector
        .renames
        .into_iter()
        .map(|(alias, original)| (original, alias))
        .collect();
    widen_by(
        &mut collector.names.modules,
        &aliased.iter().collect::<Vec<_>>(),
    );
    collector.names
}

#[derive(Default)]
struct DeclaredNamesCollector {
    names: DeclaredNames,
    /// Every `use … as`, (alias, original), resolved against the modules
    /// after the walk — the `mod` may be declared after the `use`.
    renames: Vec<(String, String)>,
    /// The first segment of every `use` path.
    use_roots: Vec<String>,
}

impl<'ast> Visit<'ast> for DeclaredNamesCollector {
    /// Operation: one map insert, then the default walk.
    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        let variants = node.variants.iter().map(|v| v.ident.to_string());
        self.names
            .enums
            .entry(node.ident.to_string())
            .or_default()
            .extend(variants);
        syn::visit::visit_item_enum(self, node);
    }

    /// Operation: one set insert, then the default walk.
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.names.modules.insert(node.ident.to_string());
        syn::visit::visit_item_mod(self, node);
    }

    /// Operation: two extends, own calls in the operands.
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        self.renames.extend(leaves(&node.tree).renames);
        self.use_roots.extend(use_roots(&node.tree));
    }

    /// What a `macro_rules!` would write out is declared as much as what is
    /// written out — see `generated_items`.
    /// Operation: iteration over the generated files, own call in the closure.
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        generated_items(node)
            .iter()
            .for_each(|file| self.visit_file(file));
    }

    /// An `extern crate` names a module, and `extern crate self as API` or
    /// `extern crate dep as API` renames one.
    /// Operation: one insert, one optional push, no own calls.
    fn visit_item_extern_crate(&mut self, node: &'ast syn::ItemExternCrate) {
        self.names.modules.insert(node.ident.to_string());
        if let Some((_, alias)) = &node.rename {
            self.renames
                .push((alias.to_string(), node.ident.to_string()));
        }
    }
}

/// The items a `macro_rules!` definition would write out: each transcriber
/// parsed as a file, when it parses. A transcriber that holds metavariables
/// or a bare expression does not, and yields nothing — the same coarse
/// treatment every macro body gets elsewhere. Anything else — a `mod`, a
/// `use`, an `enum` — is read as if it were written out, because an
/// invocation makes it exactly that: `expose!()` putting `pub use real as
/// Alias` into a module left `api::Alias()` without its binding, and `real`
/// read as dead.
/// Operation: filter over the transcribers, own calls in the closure.
pub(crate) fn generated_items(mac: &syn::Macro) -> Vec<syn::File> {
    if !mac.path.is_ident("macro_rules") {
        return Vec::new();
    }
    macro_params::transcribers(&mac.tokens)
        .into_iter()
        .filter_map(|body| syn::parse2::<syn::File>(body).ok())
        .collect()
}

/// Every `use` the generated items hold, at any depth, as one set of leaves.
/// Integration: generated items, then the collector over them.
pub(crate) fn generated_uses(mac: &syn::Macro) -> UseLeaves {
    let mut uses = GeneratedUses::default();
    generated_items(mac)
        .iter()
        .for_each(|file| uses.visit_file(file));
    uses.leaves
}

#[derive(Default)]
struct GeneratedUses {
    leaves: UseLeaves,
}

impl<'ast> Visit<'ast> for GeneratedUses {
    /// Operation: one extend, own call in the operand.
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        self.leaves.extend(leaves(&node.tree));
    }
}

/// The first segment of every path in a `use` tree: `use {a::b, C}` starts
/// at `a` and at `C`.
/// Operation: one match over the tree's top, own call in the closure.
// qual:recursive
fn use_roots(tree: &syn::UseTree) -> Vec<String> {
    match tree {
        syn::UseTree::Path(p) => vec![p.ident.to_string()],
        syn::UseTree::Name(n) => vec![n.ident.to_string()],
        syn::UseTree::Rename(r) => vec![r.ident.to_string()],
        syn::UseTree::Group(g) => g.items.iter().flat_map(use_roots).collect(),
        syn::UseTree::Glob(_) => Vec::new(),
    }
}

/// The segments of a path that a `use` may have bound — the *heads*.
///
/// The first segment resolves in scope, so a `use` in this module can be
/// behind it. The last segment of a qualified path resolves in whatever the
/// prefix names: in a *module*, a `pub use` there can be behind it
/// (`facade::Facade`, `crate::TopAlias`, `super::w()` — the ordinary facade
/// shape, and taking the first segment alone reported it dead); in a *type*,
/// it is a variant or an associated item, which no `use` can bind
/// (`Kind::Circle`, `Ordering::Less`, `Type::new()`). Which of the two the
/// qualifier is follows Rust's own convention — a CamelCase name is a type —
/// unless a module of that name is declared, because a name the workspace
/// really uses for a module must count as one however it is spelled: an
/// unrelated `enum Facade { Alias }` next to a `mod Facade` re-exporting
/// `Real as Alias` reported `Real` dead when only the enum decided — and a
/// `use shapes as Shapes` is such a name too.
/// Operation: one concatenation, own call in the operand.
pub(crate) fn heads_of(segments: &[String], modules: &HashSet<String>) -> Vec<String> {
    let first = segments.first().cloned();
    let leaf = leaf_head(segments, modules)
        .filter(|_| segments.len() > 1)
        .cloned();
    first.into_iter().chain(leaf).collect()
}

/// The *leaf* head alone: the last segment when a `use` can be behind it —
/// a single segment, or one qualified by a module. This is all a *function*
/// binding can be, since a function never sits in front of `::`; recording
/// the qualifier as a call too hid a dead `fn facade` next to `mod facade`,
/// and worse, made TQ-003 report it as untested.
/// Operation: slice shape + one predicate, no own calls.
pub(crate) fn leaf_head<'a>(
    segments: &'a [String],
    modules: &HashSet<String>,
) -> Option<&'a String> {
    match segments {
        [single] => Some(single),
        [.., qualifier, last] => {
            let names_a_type =
                qualifier.starts_with(char::is_uppercase) && !modules.contains(qualifier);
            (!names_a_type).then_some(last)
        }
        [] => None,
    }
}

/// What every `use` the walk saw implies, by the context the `use` sits in.
/// Kept as the raw leaves rather than resolved on the spot, because what a
/// member leaf implies depends on a declaration that may come later or sit in
/// another file.
#[derive(Debug, Default)]
pub(crate) struct ContextUses {
    pub(crate) production: UseLeaves,
    pub(crate) tests: UseLeaves,
}

/// "If this name is consumed, so is that one", as (name, implied), by context.
/// A list, not a map: `use x as run` in one module and `use y as run` in
/// another is ordinary Rust, and keying by the alias kept whichever came last
/// — the other original was then reported dead, depending on walk order.
#[derive(Debug, Default)]
pub(crate) struct Implications {
    pub(crate) production: Vec<(String, String)>,
    pub(crate) tests: Vec<(String, String)>,
}

impl ContextUses {
    /// Operation: one branch, one extend.
    pub(crate) fn record(&mut self, in_test: bool, leaves: UseLeaves) {
        match in_test {
            true => self.tests.extend(leaves),
            false => self.production.extend(leaves),
        }
    }

    /// The renames alone — all a `use` can imply about a *function*, since a
    /// function has no variants.
    /// Operation: two clones, no own calls.
    pub(crate) fn renames(&self) -> Implications {
        Implications {
            production: self.production.renames.clone(),
            tests: self.tests.renames.clone(),
        }
    }

    /// Renames plus what the member and glob leaves imply about the enums in
    /// `enums`: consuming a variant consumes its enum, and after
    /// `use Shape::{Circle, Square}` or `use Shape::*` the enum's own name
    /// never appears again, so that import is the only place the two meet.
    /// The parent has to be a declared enum with that variant — `use
    /// inner::Shape::foo` with a *module* `Shape` says nothing about a struct
    /// that happens to share the name. Recording every prefix as a reference
    /// did both wrong: an unconsumed `use Shape::*` kept the enum alive, and
    /// a module segment vouched for a same-named type. The parent may itself
    /// be an alias (`use Shape as Form; use Form::*`), so it is resolved
    /// through the renames first — those *its own context* can see: a
    /// production `use` never sees a `#[cfg(test)]` rename, and resolving it
    /// through one let a test alias keep a production enum alive that rustc
    /// reports unused. A test `use` sees both, like everything else in tests.
    /// Operation: two delegations, the test alias list built by concatenation.
    pub(crate) fn implications(&self, enums: &EnumVariants) -> Implications {
        let production: Vec<&(String, String)> = self.production.renames.iter().collect();
        let tests: Vec<&(String, String)> = production
            .iter()
            .copied()
            .chain(self.tests.renames.iter())
            .collect();
        Implications {
            production: implied_by(&self.production, enums, &production),
            tests: implied_by(&self.tests, enums, &tests),
        }
    }
}

/// Operation: three chained selections over the leaves, own calls in the closures.
fn implied_by(
    leaves: &UseLeaves,
    enums: &EnumVariants,
    aliases: &[&(String, String)],
) -> Vec<(String, String)> {
    let enums_behind = |parent: &String| {
        let mut names = HashSet::from([parent.clone()]);
        widen_by(&mut names, aliases);
        names.into_iter().filter(|n| enums.contains_key(n))
    };
    let member = |(name, parent): &(String, String)| {
        enums_behind(parent)
            .filter(|e| enums[e].contains(name))
            .map(|e| (name.clone(), e))
            .collect::<Vec<_>>()
    };
    let glob = |parent: &String| {
        enums_behind(parent)
            .flat_map(|e| enums[&e].iter().map(move |v| (v.clone(), e.clone())))
            .collect::<Vec<_>>()
    };
    leaves
        .renames
        .iter()
        .cloned()
        .chain(leaves.members.iter().flat_map(member))
        .chain(leaves.globs.iter().flat_map(glob))
        .collect()
}

impl ContextRefs {
    /// Bring every name the *heads* imply into each set, each with the
    /// implications its context can see. Only a head — a name used
    /// unqualified — can consume what a `use` bound: `Kind::Circle` resolves
    /// through `Kind` and says nothing about the `Circle` a `use Shape::*`
    /// brought in, and `m::perform()` is not the `perform` a rename bound.
    /// Resolving against every mention instead kept a glob-imported enum alive
    /// whenever any other enum's same-named variant was used anywhere.
    ///
    /// A `use` that only exists inside a `#[cfg(test)]` module can only ever
    /// serve tests, however a production name may happen to coincide with the
    /// alias — resolving such a rename against the production set reported a
    /// working `qual:test_helper` as spent. The other direction is open on
    /// purpose: a production `use` is visible to a test module that says
    /// `use super::*`, so the test set is widened by both lists.
    /// Over-approximating there can only turn `uncalled` into `testonly`,
    /// never invent a finding.
    /// Operation: two delegations, the second list built by concatenation.
    pub(crate) fn widen_by_implications(&mut self, heads: &ContextRefs, implied: &Implications) {
        let production: Vec<&(String, String)> = implied.production.iter().collect();
        let tests: Vec<&(String, String)> = production
            .iter()
            .copied()
            .chain(implied.tests.iter())
            .collect();
        widen_from(&mut self.production, &heads.production, &production);
        widen_from(&mut self.tests, &heads.tests, &tests);
    }
}

/// Bring into `set` every name the `heads` imply.
///
/// A `use X as Y` is exposure like any other `use` — it keeps nothing alive by
/// itself. But once something *calls* `Y`, the call site can only ever say `Y`,
/// and the declaration is named `X`. This is the one shape where consumption
/// and the declaration's name cannot meet without the `use` in between (a
/// variant import is the other: `Circle` is said, `Shape` is meant), so the
/// pairs are applied afterwards to each set on its own (see
/// `ContextRefs::widen_by_implications` for the contexts), starting from the
/// heads alone, and a `use` nobody consumes still brings nothing back.
/// Operation: one fixpoint, then the merge; own call in the operand.
fn widen_from(set: &mut HashSet<String>, heads: &HashSet<String>, implied: &[&(String, String)]) {
    let mut reached = heads.clone();
    widen_by(&mut reached, implied);
    set.extend(reached);
}

/// Bring into `set` every name it implies. Runs to a fixpoint, so
/// `use a as b; use b as c;` resolves through both hops.
/// Operation: fixpoint over the pairs, no own calls.
fn widen_by(set: &mut HashSet<String>, implied: &[&(String, String)]) {
    loop {
        let reached: Vec<String> = implied
            .iter()
            .filter(|(name, implied)| set.contains(name) && !set.contains(implied))
            .map(|(_, implied)| implied.clone())
            .collect();
        if reached.is_empty() {
            return;
        }
        set.extend(reached);
    }
}
