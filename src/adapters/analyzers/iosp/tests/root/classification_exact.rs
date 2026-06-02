use super::*;

const CLASSIFICATION_EXACT_CASES: &[(&str, &str, &str, Classification)] = &[
    (
        "pure integration (two own free-fn calls)",
        r#"
            fn helper_a() {}
            fn helper_b() {}
            fn integrator() {
                helper_a();
                helper_b();
            }
            "#,
        "integrator",
        Classification::Integration,
    ),
    (
        "pure operation (branching logic, no own calls)",
        r#"
            fn operation(x: i32) -> &'static str {
                let _y = x;
                if _y > 0 {
                    "positive"
                } else {
                    "non-positive"
                }
            }
            "#,
        "operation",
        Classification::Operation,
    ),
    (
        "trivial empty body",
        "fn f() {}",
        "f",
        Classification::Trivial,
    ),
    (
        "trivial single return",
        "fn f() -> i32 { 42 }",
        "f",
        Classification::Trivial,
    ),
    (
        "trivial getter",
        r#"
            struct Foo { x: i32 }
            impl Foo {
                fn get_x(&self) -> i32 { self.x }
            }
            "#,
        "get_x",
        Classification::Trivial,
    ),
    (
        "single statement with own call → integration",
        r#"
            fn helper() {}
            fn f() { helper() }
            "#,
        "f",
        Classification::Integration,
    ),
    (
        "single statement with logic → operation",
        "fn f(x: i32) -> i32 { if x > 0 { 1 } else { 0 } }",
        "f",
        Classification::Operation,
    ),
    (
        "self-method calls → integration",
        r#"
            struct MyStruct;
            impl MyStruct {
                fn do_work(&self) {}
                fn orchestrate(&self) {
                    self.do_work();
                    self.do_work();
                }
            }
            "#,
        "orchestrate",
        Classification::Integration,
    ),
    (
        "external method calls → operation",
        r#"
            fn operation_fn() {
                let mut v = Vec::new();
                if v.is_empty() {
                    v.push(1);
                }
            }
            "#,
        "operation_fn",
        Classification::Operation,
    ),
    (
        "own free-fn calls → integration",
        r#"
            fn step_a() {}
            fn step_b() {}
            fn orchestrate() {
                step_a();
                step_b();
            }
            "#,
        "orchestrate",
        Classification::Integration,
    ),
    (
        "same-type path ctor calls → integration",
        r#"
            struct MyType;
            impl MyType {
                fn create() -> Self { MyType }
            }
            fn f() {
                let _a = MyType::create();
                let _b = MyType::create();
            }
            "#,
        "f",
        Classification::Integration,
    ),
];

#[test]
fn classification_exact() {
    for (label, code, fn_name, expected) in CLASSIFICATION_EXACT_CASES {
        assert_eq!(&classify(code, fn_name), expected, "case {label}");
    }
}
