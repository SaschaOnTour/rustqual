use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Foo {
    pub name: String,
}
