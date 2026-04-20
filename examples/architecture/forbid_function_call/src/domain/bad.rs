pub fn make_boxed() -> Box<i32> {
    let value = 42;

    Box::new(value)
}
