use std::collections::{HashMap, HashSet};

use syn::visit::Visit;
use syn::{File, ImplItem, TraitItem};

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
    /// Methods grouped by parent type name: `"Config" → {"load", "save"}`.
    pub methods_by_type: HashMap<String, HashSet<String>>,
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
            methods_by_type: HashMap::new(),
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
            methods_by_type: collector.methods_by_type,
        }
    }

    /// Is `name` an own *function* call (path-style: `func()` or `Type::func()`)?
    ///
    /// - Single segment (`classify_function`): checks `functions`
    /// - Multi-segment (`Config::load`): checks if first segment is in `types`
    /// - `Self::method`: checks `methods` excluding trait-only
    /// - PascalCase final segment: enum variant constructor, not a call
    ///
    /// Operation: if-let + comparison logic, no own calls.
    pub fn is_own_function(&self, name: &str) -> bool {
        if let Some((prefix, method)) = name.split_once("::") {
            if prefix == "Self" {
                return self.methods.contains(method) && !self.trait_only_methods.contains(method);
            }
            self.types.contains(prefix)
                && !method.starts_with(char::is_uppercase)
                && !self.trait_only_methods.contains(method)
        } else {
            self.functions.contains(name)
        }
    }

    /// Is `name` an own *method* call (dot-style: `.method()`)?
    /// Fallback for receivers with unknown type.
    /// Operation: boolean logic, no own calls.
    pub fn is_own_method(&self, name: &str) -> bool {
        self.methods.contains(name)
            && !self.trivial_methods.contains(name)
            && !self.trait_only_methods.contains(name)
    }

    /// Is `.method()` an own call when called on `self` inside an impl of `parent_type`?
    /// Layer 1: checks if this specific type defines the method.
    /// Operation: lookup logic, no own calls.
    pub fn is_own_self_method(&self, method: &str, parent_type: &str) -> bool {
        self.methods_by_type
            .get(parent_type)
            .map(|m| m.contains(method))
            .unwrap_or(false)
            && !self.trivial_methods.contains(method)
            && !self.trait_only_methods.contains(method)
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
            "len"
                | "is_empty"
                | "clone"
                | "as_ref"
                | "as_mut"
                | "as_str"
                | "to_owned"
                | "to_string"
                | "borrow"
                | "borrow_mut"
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
    /// Methods grouped by parent type.
    methods_by_type: HashMap<String, HashSet<String>>,
}

impl<'ast> Visit<'ast> for ScopeCollector {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.functions.insert(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let type_name = if let syn::Type::Path(tp) = &*node.self_ty {
            tp.path.segments.last().map(|seg| {
                self.types.insert(seg.ident.to_string());
                seg.ident.to_string()
            })
        } else {
            None
        };
        let is_trait_impl = node.trait_.is_some();
        for item in &node.items {
            if let ImplItem::Fn(method) = item {
                let name = method.sig.ident.to_string();
                self.methods.insert(name.clone());
                if let Some(ref tn) = type_name {
                    self.methods_by_type
                        .entry(tn.clone())
                        .or_default()
                        .insert(name.clone());
                }
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
