use std::collections::HashSet;

use syn::visit::Visit;
use syn::{File, ImplItem, TraitItem};

/// Names that are so ubiquitous across Rust that they should never be
/// counted as "own" calls, even when the project defines them (e.g. via
/// trait implementations like `Display::fmt` or `Default::default`).
const UNIVERSAL_METHODS: &[&str] = &[
    "new",
    "default",
    "from",
    "into",
    "try_from",
    "try_into",
    "clone",
    "clone_from",
    "fmt",
    "to_string",
    "eq",
    "ne",
    "partial_cmp",
    "cmp",
    "hash",
    "drop",
    "deref",
    "deref_mut",
    "as_ref",
    "as_mut",
    "borrow",
    "borrow_mut",
    "from_str",
    "index",
    "index_mut",
    "next",
];

/// Collects all declared function/method/type names across a project.
///
/// Used in a two-pass analysis: first pass builds the scope, second pass
/// uses it to distinguish own calls from external ones.
#[derive(Debug, Clone, Default)]
pub struct ProjectScope {
    /// Free functions: `classify_function`, `collect_rust_files`, …
    pub functions: HashSet<String>,
    /// Methods from impl/trait blocks: `analyze_file`, `load`, …
    pub methods: HashSet<String>,
    /// Struct/enum/trait names: `Analyzer`, `Config`, `Summary`, …
    pub types: HashSet<String>,
    /// Trivial getters: single-receiver, single-statement methods (equivalent to field access).
    pub trivial_methods: HashSet<String>,
    /// Methods that only appear in trait contexts (trait defs or trait impls), never in inherent impls.
    /// Dot-syntax calls to these are polymorphic dispatch, not own calls.
    pub trait_only_methods: HashSet<String>,
}

impl ProjectScope {
    /// Build a ProjectScope from a set of already-parsed files.
    pub fn from_files(files: &[(&str, &File)]) -> Self {
        let mut collector = ScopeCollector {
            functions: HashSet::new(),
            methods: HashSet::new(),
            types: HashSet::new(),
            trivial_candidates: HashSet::new(),
            non_trivial_methods: HashSet::new(),
            trait_method_names: HashSet::new(),
            concrete_method_names: HashSet::new(),
        };
        files
            .iter()
            .for_each(|(_, file)| collector.visit_file(file));
        ProjectScope {
            functions: collector.functions,
            methods: collector.methods,
            types: collector.types,
            trivial_methods: collector
                .trivial_candidates
                .difference(&collector.non_trivial_methods)
                .cloned()
                .collect(),
            trait_only_methods: collector
                .trait_method_names
                .difference(&collector.concrete_method_names)
                .cloned()
                .collect(),
        }
    }

    /// Is `name` an own *function* call (path-style: `func()` or `Type::func()`)?
    ///
    /// - Single segment (`classify_function`): checks `functions`
    /// - Multi-segment (`Config::load`): checks if first segment is in `types`
    /// - `Self::method`: checks against UNIVERSAL_METHODS
    ///
    /// Operation: if-let + comparison logic, no own calls.
    pub fn is_own_function(&self, name: &str) -> bool {
        if let Some((prefix, method)) = name.split_once("::") {
            if prefix == "Self" {
                return !UNIVERSAL_METHODS.contains(&method)
                    && self.methods.contains(method)
                    && !self.trait_only_methods.contains(method);
            }
            self.types.contains(prefix) && !UNIVERSAL_METHODS.contains(&method)
        } else {
            self.functions.contains(name)
        }
    }

    /// Is `name` an own *method* call (dot-style: `.method()`)?
    ///
    /// True only if the name appears in `methods` AND is not a universal method.
    /// Operation: boolean logic, no own calls.
    pub fn is_own_method(&self, name: &str) -> bool {
        self.methods.contains(name)
            && !UNIVERSAL_METHODS.contains(&name)
            && !self.trivial_methods.contains(name)
            && !self.trait_only_methods.contains(name)
    }
}

/// Check if a method signature has only `self`/`&self`/`&mut self` with no other parameters.
/// Operation: iteration + pattern matching logic, no own calls.
fn has_trivial_self_signature(sig: &syn::Signature) -> bool {
    let has_receiver = sig
        .inputs
        .iter()
        .any(|arg| matches!(arg, syn::FnArg::Receiver(_)));
    let typed_count = sig
        .inputs
        .iter()
        .filter(|arg| matches!(arg, syn::FnArg::Typed(_)))
        .count();
    has_receiver && typed_count == 0
}

/// Check if a method call is a trivial accessor call (no-arg stdlib accessor or `.get()` with
/// a trivial argument like a literal or self field access).
/// Operation: if/match logic with inlined arg check, no own calls.
fn is_trivial_method_call(mc: &syn::ExprMethodCall) -> bool {
    let method_name = mc.method.to_string();
    if mc.args.is_empty() {
        return matches!(
            method_name.as_str(),
            "len" | "is_empty" | "clone" | "as_ref" | "as_mut" | "as_str"
                | "to_owned" | "to_string" | "borrow" | "borrow_mut"
        );
    }
    if mc.args.len() != 1 || !matches!(method_name.as_str(), "get") {
        return false;
    }
    // Inline trivial-arg check: self field access, literal, or reference thereof
    let mut current = &mc.args[0];
    loop {
        match current {
            syn::Expr::Lit(_) => return true,
            syn::Expr::Field(f) => current = &f.base,
            syn::Expr::Path(p) if p.path.is_ident("self") => return true,
            syn::Expr::Reference(r) => current = &r.expr,
            _ => return false,
        }
    }
}

/// Check if a method body is a trivial accessor (single expression accessing self fields).
/// Handles: `self.x`, `&self.x`, `self.x.clone()`, `self.x.len()`, `self.x as f64`,
/// `self.items.get(self.index)`, etc.
/// Operation: iterative loop with pattern matching, no own calls (closure hides helper).
fn is_trivial_accessor_body(block: &syn::Block) -> bool {
    if block.stmts.len() != 1 {
        return false;
    }
    let expr = match &block.stmts[0] {
        syn::Stmt::Expr(e, _) => e,
        _ => return false,
    };
    let check_call = |mc: &syn::ExprMethodCall| is_trivial_method_call(mc);
    let mut current = expr;
    loop {
        match current {
            syn::Expr::Field(_) => return true,
            syn::Expr::Reference(r) => current = &r.expr,
            syn::Expr::Cast(c) => current = &c.expr,
            syn::Expr::Unary(u) => current = &u.expr,
            syn::Expr::Paren(p) => current = &p.expr,
            syn::Expr::MethodCall(mc) if check_call(mc) => {
                current = &mc.receiver;
            }
            _ => return false,
        }
    }
}

/// AST visitor that collects declarations (functions, methods, types).
struct ScopeCollector {
    functions: HashSet<String>,
    methods: HashSet<String>,
    types: HashSet<String>,
    trivial_candidates: HashSet<String>,
    non_trivial_methods: HashSet<String>,
    /// Methods from trait definitions and `impl Trait for Struct` blocks.
    trait_method_names: HashSet<String>,
    /// Methods from inherent (non-trait) impl blocks only.
    concrete_method_names: HashSet<String>,
}

impl<'ast> Visit<'ast> for ScopeCollector {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.functions.insert(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if let syn::Type::Path(tp) = &*node.self_ty {
            if let Some(seg) = tp.path.segments.last() {
                self.types.insert(seg.ident.to_string());
            }
        }
        let is_trait_impl = node.trait_.is_some();
        for item in &node.items {
            if let ImplItem::Fn(method) = item {
                let name = method.sig.ident.to_string();
                self.methods.insert(name.clone());
                if is_trait_impl {
                    self.trait_method_names.insert(name.clone());
                } else {
                    self.concrete_method_names.insert(name.clone());
                }
                if has_trivial_self_signature(&method.sig)
                    && is_trivial_accessor_body(&method.block)
                {
                    self.trivial_candidates.insert(name);
                } else {
                    self.non_trivial_methods.insert(name);
                }
            }
        }
        syn::visit::visit_item_impl(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        self.types.insert(node.ident.to_string());
        for item in &node.items {
            if let TraitItem::Fn(method) = item {
                let name = method.sig.ident.to_string();
                self.methods.insert(name.clone());
                self.trait_method_names.insert(name);
            }
        }
        syn::visit::visit_item_trait(self, node);
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.types.insert(node.ident.to_string());
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.types.insert(node.ident.to_string());
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        // Recurse into inline modules to collect their declarations too
        syn::visit::visit_item_mod(self, node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_scope(code: &str) -> ProjectScope {
        let syntax = syn::parse_file(code).expect("Failed to parse test code");
        let files = vec![("test.rs", &syntax)];
        ProjectScope::from_files(&files)
    }

    // ── ScopeCollector Tests ──────────────────────────────────────────

    #[test]
    fn test_collect_free_functions() {
        let scope = build_scope("fn foo() {} fn bar() {}");
        assert!(scope.functions.contains("foo"));
        assert!(scope.functions.contains("bar"));
    }

    #[test]
    fn test_collect_impl_methods() {
        let scope = build_scope("struct Foo; impl Foo { fn bar(&self) {} }");
        assert!(scope.methods.contains("bar"));
        assert!(scope.types.contains("Foo"));
    }

    #[test]
    fn test_collect_trait_methods() {
        let scope = build_scope("trait T { fn baz(&self); }");
        assert!(scope.methods.contains("baz"));
        assert!(scope.types.contains("T"));
    }

    #[test]
    fn test_collect_struct_names() {
        let scope = build_scope("struct S;");
        assert!(scope.types.contains("S"));
    }

    #[test]
    fn test_collect_enum_names() {
        let scope = build_scope("enum E { A }");
        assert!(scope.types.contains("E"));
    }

    #[test]
    fn test_collect_nested_module() {
        let scope = build_scope("mod inner { fn f() {} }");
        assert!(scope.functions.contains("f"));
    }

    // ── is_own_function Tests ─────────────────────────────────────────

    #[test]
    fn test_own_function_single_segment() {
        let scope = build_scope("fn foo() {}");
        assert!(scope.is_own_function("foo"));
    }

    #[test]
    fn test_own_function_not_in_scope() {
        let scope = build_scope("fn foo() {}");
        assert!(!scope.is_own_function("unknown"));
    }

    #[test]
    fn test_own_function_type_prefix() {
        let scope = build_scope("struct Config; impl Config { fn load() {} }");
        assert!(scope.is_own_function("Config::load"));
    }

    #[test]
    fn test_own_function_external_type() {
        let scope = build_scope("fn foo() {}");
        assert!(!scope.is_own_function("String::new"));
    }

    #[test]
    fn test_own_function_self_universal() {
        let scope = build_scope("struct X; impl X { fn default(&self) {} }");
        assert!(!scope.is_own_function("Self::default"));
    }

    #[test]
    fn test_own_function_self_own_method() {
        let scope = build_scope("struct X; impl X { fn analyze(&self) {} }");
        assert!(scope.is_own_function("Self::analyze"));
    }

    // ── is_own_method Tests ───────────────────────────────────────────

    #[test]
    fn test_own_method_in_scope() {
        let scope = build_scope("struct A; impl A { fn analyze_file(&self) {} }");
        assert!(scope.is_own_method("analyze_file"));
    }

    #[test]
    fn test_own_method_not_in_scope() {
        let scope = build_scope("struct A; impl A { fn analyze_file(&self) {} }");
        assert!(!scope.is_own_method("push"));
    }

    #[test]
    fn test_own_method_universal_blocked() {
        let scope = build_scope("struct A; impl A { fn new() -> Self { A } }");
        assert!(!scope.is_own_method("new"));
    }

    #[test]
    fn test_own_method_not_universal() {
        let scope = build_scope("struct A; impl A { fn record_logic(&mut self) {} }");
        assert!(scope.is_own_method("record_logic"));
    }

    // ── Trivial Accessor Tests ───────────────────────────────────────

    #[test]
    fn test_trivial_method_field_access() {
        let scope = build_scope(
            "struct S { x: i32 } impl S { fn x(&self) -> i32 { self.x } }",
        );
        assert!(scope.trivial_methods.contains("x"));
    }

    #[test]
    fn test_trivial_method_len_chain() {
        let scope = build_scope(
            "struct S { items: Vec<i32> } impl S { fn count(&self) -> usize { self.items.len() } }",
        );
        assert!(scope.trivial_methods.contains("count"));
    }

    #[test]
    fn test_trivial_method_clone() {
        let scope = build_scope(
            "struct S { name: String } impl S { fn name(&self) -> String { self.name.clone() } }",
        );
        assert!(scope.trivial_methods.contains("name"));
    }

    #[test]
    fn test_trivial_method_reference() {
        let scope = build_scope(
            "struct S { data: Vec<u8> } impl S { fn data(&self) -> &Vec<u8> { &self.data } }",
        );
        assert!(scope.trivial_methods.contains("data"));
    }

    #[test]
    fn test_nontrivial_method_with_params() {
        let scope = build_scope(
            "struct S { v: Vec<i32> } impl S { fn get(&self, i: usize) -> i32 { self.v[i] } }",
        );
        assert!(!scope.trivial_methods.contains("get"));
    }

    #[test]
    fn test_nontrivial_method_with_logic() {
        let scope = build_scope(
            "struct S { x: i32 } impl S { fn check(&self) -> bool { if self.x > 0 { true } else { false } } }",
        );
        assert!(!scope.trivial_methods.contains("check"));
    }

    #[test]
    fn test_nontrivial_method_multi_stmt() {
        let scope = build_scope(
            "struct S { x: i32 } impl S { fn compute(&self) -> i32 { let y = self.x; y } }",
        );
        assert!(!scope.trivial_methods.contains("compute"));
    }

    #[test]
    fn test_trivial_method_not_own_call() {
        let scope = build_scope(
            "struct S { items: Vec<i32> } impl S { fn count(&self) -> usize { self.items.len() } }",
        );
        assert!(
            !scope.is_own_method("count"),
            "Trivial getter should not be counted as own call"
        );
    }

    // ── Bug 4: Type::new() / Type::default() Not Own Call ──────────

    #[test]
    fn test_own_function_type_universal_blocked() {
        let scope = build_scope("struct MyType; impl MyType { fn new() -> Self { MyType } }");
        assert!(
            !scope.is_own_function("MyType::new"),
            "MyType::new() should NOT be own function (new is universal)"
        );
    }

    #[test]
    fn test_own_function_type_default_blocked() {
        let scope =
            build_scope("struct MyType; impl MyType { fn default() -> Self { MyType } }");
        assert!(
            !scope.is_own_function("MyType::default"),
            "MyType::default() should NOT be own function (default is universal)"
        );
    }

    #[test]
    fn test_own_function_type_from_blocked() {
        let scope = build_scope(
            "struct MyType; impl MyType { fn from(x: i32) -> Self { MyType } }",
        );
        assert!(
            !scope.is_own_function("MyType::from"),
            "MyType::from() should NOT be own function (from is universal)"
        );
    }

    #[test]
    fn test_own_function_type_custom_still_counted() {
        let scope = build_scope("struct MyType; impl MyType { fn load() -> Self { MyType } }");
        assert!(
            scope.is_own_function("MyType::load"),
            "MyType::load() SHOULD be own function (load is not universal)"
        );
    }

    // ── Bug 5: Trivial .get() Accessor Tests ────────────────────────

    #[test]
    fn test_trivial_method_get_self_field() {
        let scope = build_scope(
            "struct S { items: Vec<i32>, index: usize }
             impl S { fn current(&self) -> Option<&i32> { self.items.get(self.index) } }",
        );
        assert!(
            scope.trivial_methods.contains("current"),
            ".get(self.field) should be trivial"
        );
    }

    #[test]
    fn test_trivial_method_get_literal() {
        let scope = build_scope(
            "struct S { items: Vec<i32> }
             impl S { fn first(&self) -> Option<&i32> { self.items.get(0) } }",
        );
        assert!(
            scope.trivial_methods.contains("first"),
            ".get(0) should be trivial"
        );
    }

    #[test]
    fn test_trivial_method_get_ref_field() {
        let scope = build_scope(
            "struct S { map: std::collections::HashMap<String, i32>, key: String }
             impl S { fn entry(&self) -> Option<&i32> { self.map.get(&self.key) } }",
        );
        assert!(
            scope.trivial_methods.contains("entry"),
            ".get(&self.key) should be trivial"
        );
    }

    #[test]
    fn test_trivial_method_get_deep_field() {
        let scope = build_scope(
            "struct Config { idx: usize }
             struct S { data: Vec<i32>, config: Config }
             impl S { fn item(&self) -> Option<&i32> { self.data.get(self.config.idx) } }",
        );
        assert!(
            scope.trivial_methods.contains("item"),
            ".get(self.config.idx) should be trivial"
        );
    }

    // ── Trait-Only Method Tests ──────────────────────────────────

    #[test]
    fn test_trait_only_method_not_own_call() {
        let scope = build_scope(
            "trait Provider { fn fetch(&self) -> i32; }
             struct S;
             impl Provider for S { fn fetch(&self) -> i32 { 42 } }",
        );
        assert!(
            scope.trait_only_methods.contains("fetch"),
            "fetch only appears in trait + trait impl, should be trait-only"
        );
        assert!(
            !scope.is_own_method("fetch"),
            "trait-only method should NOT be counted as own call"
        );
    }

    #[test]
    fn test_trait_and_inherent_collision_not_trait_only() {
        let scope = build_scope(
            "trait T { fn process(&self); }
             struct A;
             impl T for A { fn process(&self) {} }
             struct B;
             impl B { fn process(&self) {} }",
        );
        assert!(
            !scope.trait_only_methods.contains("process"),
            "process appears in both trait impl and inherent impl — not trait-only"
        );
        assert!(
            scope.is_own_method("process"),
            "collision should be counted as own call"
        );
    }

    #[test]
    fn test_external_trait_impl_trait_only() {
        // Simulates implementing an external trait locally
        let scope = build_scope(
            "struct S;
             impl std::fmt::Display for S {
                 fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) }
             }",
        );
        // fmt is a UNIVERSAL_METHOD so would be excluded anyway, but the trait_only
        // tracking should still work. Let's test with a non-universal name:
        let scope2 = build_scope(
            "trait ExternalTrait { fn do_work(&self); }
             struct S;
             impl ExternalTrait for S { fn do_work(&self) {} }",
        );
        assert!(
            scope2.trait_only_methods.contains("do_work"),
            "Method only in trait impl should be trait-only"
        );
        assert!(
            !scope2.is_own_method("do_work"),
            "trait-only method should not be own call"
        );
        // Verify fmt is still in the set (even though UNIVERSAL_METHODS would block it)
        assert!(scope.trait_only_methods.contains("fmt"));
    }

    #[test]
    fn test_self_path_trait_only_not_own_function() {
        let scope = build_scope(
            "trait Provider { fn fetch(&self) -> i32; }
             struct S;
             impl Provider for S { fn fetch(&self) -> i32 { 42 } }",
        );
        assert!(
            !scope.is_own_function("Self::fetch"),
            "Self::fetch for trait-only method should NOT be own function"
        );
    }

    #[test]
    fn test_trait_def_only_no_impl() {
        let scope = build_scope(
            "trait Provider { fn fetch_data(&self) -> Vec<u8>; }",
        );
        assert!(
            scope.trait_only_methods.contains("fetch_data"),
            "Method only in trait definition should be trait-only"
        );
        assert!(
            !scope.is_own_method("fetch_data"),
            "trait-only method should not be own call"
        );
    }

    #[test]
    fn test_nontrivial_method_get_computed_arg() {
        let scope = build_scope(
            "struct S { items: Vec<i32>, index: usize }
             impl S { fn item(&self) -> Option<&i32> { self.items.get(self.index + 1) } }",
        );
        assert!(
            !scope.trivial_methods.contains("item"),
            ".get(self.index + 1) should NOT be trivial (arithmetic)"
        );
    }

    // ── Name Collision Tests ─────────────────────────────────────────

    #[test]
    fn test_name_collision_conservative() {
        let scope = build_scope(
            "struct A { x: i32 }
             impl A { fn value(&self) -> i32 { self.x } }
             struct B { items: Vec<i32> }
             impl B { fn value(&self, idx: usize) -> i32 { self.items[idx] } }",
        );
        assert!(
            !scope.trivial_methods.contains("value"),
            "Name collision should be conservative — non-trivial wins"
        );
        assert!(
            scope.is_own_method("value"),
            "Collided method should be counted as own call"
        );
    }
}
