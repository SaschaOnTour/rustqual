use crate::adapters::analyzers::tq::dispatch::visitor_dispatch_edges;

/// Parse one source string into the `(path, source, File)` triple the analyzers
/// consume.
fn parsed(src: &str) -> Vec<(String, String, syn::File)> {
    let file = syn::parse_str::<syn::File>(src).expect("test source parses");
    vec![("lib.rs".to_string(), src.to_string(), file)]
}

/// A function that constructs a visitor and drives it via a method call must
/// gain an edge to the helper methods that visitor's overrides invoke.
#[test]
fn method_drive_links_driver_to_visitor_helpers() {
    let src = r#"
        struct V { count: u32 }
        impl<'ast> syn::visit::Visit<'ast> for V {
            fn visit_expr(&mut self, e: &syn::Expr) {
                self.record(e);
                syn::visit::visit_expr(self, e);
            }
        }
        impl V {
            fn record(&self, _e: &syn::Expr) {}
        }
        fn driver(body: &syn::Block) {
            let mut v = V { count: 0 };
            v.visit_block(body);
        }
    "#;
    let edges = visitor_dispatch_edges(&parsed(src));
    assert!(
        edges
            .iter()
            .any(|(from, tos)| from == "driver" && tos.iter().any(|t| t == "record")),
        "driver→record edge missing; got {edges:?}"
    );
}

/// The `syn::visit::visit_block(&mut v, body)` free-function drive form, with the
/// visitor built by a `::new()` constructor, resolves the same way.
#[test]
fn free_fn_drive_resolves_constructor_binding() {
    let src = r#"
        struct N;
        impl N {
            fn new() -> Self { N }
            fn helper(&self) {}
        }
        impl<'ast> syn::visit::Visit<'ast> for N {
            fn visit_stmt(&mut self, s: &syn::Stmt) {
                self.helper();
                syn::visit::visit_stmt(self, s);
            }
        }
        fn run(body: &syn::Block) {
            let mut n = N::new();
            syn::visit::visit_block(&mut n, body);
        }
    "#;
    let edges = visitor_dispatch_edges(&parsed(src));
    assert!(
        edges
            .iter()
            .any(|(from, tos)| from == "run" && tos.iter().any(|t| t == "helper")),
        "run→helper edge missing; got {edges:?}"
    );
}

/// The shared `visit_all_files(parsed, &mut collector)` forwarder drives its
/// second argument.
#[test]
fn forwarder_drive_resolves_second_argument() {
    let src = r#"
        struct C;
        impl C {
            fn default() -> Self { C }
            fn tally(&self) {}
        }
        impl<'ast> syn::visit::Visit<'ast> for C {
            fn visit_item_use(&mut self, u: &syn::ItemUse) {
                self.tally();
                syn::visit::visit_item_use(self, u);
            }
        }
        fn analyze(files: &[(String, String, syn::File)]) {
            let mut collector = C::default();
            visit_all_files(files, &mut collector);
        }
    "#;
    let edges = visitor_dispatch_edges(&parsed(src));
    assert!(
        edges
            .iter()
            .any(|(from, tos)| from == "analyze" && tos.iter().any(|t| t == "tally")),
        "analyze→tally edge missing; got {edges:?}"
    );
}

/// A type that is never driven produces no edges — testedness must not flow to a
/// visitor no caller launches.
#[test]
fn undriven_visitor_yields_no_edges() {
    let src = r#"
        struct Unused;
        impl<'ast> syn::visit::Visit<'ast> for Unused {
            fn visit_expr(&mut self, e: &syn::Expr) {
                self.lonely();
                syn::visit::visit_expr(self, e);
            }
        }
        impl Unused {
            fn lonely(&self) {}
        }
        fn unrelated() {
            let _x = 1;
        }
    "#;
    let edges = visitor_dispatch_edges(&parsed(src));
    assert!(
        edges.is_empty(),
        "no drive-site exists, so no edges expected; got {edges:?}"
    );
}

/// `collect_split(parsed, cfg_test_files, &mut collector)` drives its third
/// argument. Registering a forwarder here is what keeps the driver→visitor edge
/// visible when the per-file loop is shared instead of written out in each
/// collector — without it, sharing the loop silently costs the edge and every
/// visitor helper reads as untested.
#[test]
fn forwarder_drive_resolves_third_argument() {
    let src = r#"
        struct C;
        impl C {
            fn default() -> Self { C }
            fn record(&self) {}
        }
        impl<'ast> syn::visit::Visit<'ast> for C {
            fn visit_item_use(&mut self, u: &syn::ItemUse) {
                self.record();
                syn::visit::visit_item_use(self, u);
            }
        }
        fn analyze(files: &[(String, String, syn::File)], skip: &HashSet<String>) {
            collect_split(files, skip, &mut C::default())
        }
    "#;
    let edges = visitor_dispatch_edges(&parsed(src));
    assert!(
        edges
            .iter()
            .any(|(from, tos)| from == "analyze" && tos.iter().any(|t| t == "record")),
        "analyze→record edge missing; got {edges:?}"
    );
}
