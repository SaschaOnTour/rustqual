// Golden-example violation: domain-layer code calling `.unwrap()` both in
// direct dot-notation and UFCS form. The `no_panic_helpers_in_production`
// Architecture rule must flag both.

fn direct_call() {
    let x: Option<i32> = Some(1);
    x.unwrap();
}

fn ufcs_call() {
    let x: Option<i32> = Some(2);
    Option::unwrap(x);
}
