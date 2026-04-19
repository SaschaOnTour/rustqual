//! Contract tests for the `SourceLoader` port.
//!
//! These tests verify the port's *shape*: object-safety, Send + Sync
//! supertraits, and the semantic error set. Concrete loader impls live
//! in adapters and carry their own behaviour tests.

use crate::domain::SourceUnit;
use crate::ports::{LoadError, SourceLoader};
use std::path::{Path, PathBuf};

/// Trivial fake loader: returns what it was constructed with.
struct FakeLoader {
    units: Vec<SourceUnit>,
}

impl SourceLoader for FakeLoader {
    fn load(&self, _root: &Path) -> Result<Vec<SourceUnit>, LoadError> {
        Ok(self.units.clone())
    }
}

#[test]
fn port_is_object_safe() {
    // If `dyn SourceLoader` compiles, the trait is object-safe.
    let fake = FakeLoader { units: vec![] };
    let _boxed: Box<dyn SourceLoader> = Box::new(fake);
}

#[test]
fn port_requires_send_and_sync() {
    // Compile-time assertion via trait-object coercion: if FakeLoader is
    // not Send + Sync, this line does not compile.
    let _: Box<dyn Send + Sync> = Box::new(FakeLoader { units: vec![] });
}

#[test]
fn fake_loader_returns_injected_units() {
    let units = vec![
        SourceUnit::new(PathBuf::from("a.rs"), "fn a() {}".into()),
        SourceUnit::new(PathBuf::from("b.rs"), "fn b() {}".into()),
    ];
    let loader = FakeLoader {
        units: units.clone(),
    };
    let loaded = loader.load(Path::new("/ignored")).unwrap();
    assert_eq!(loaded, units);
}

#[test]
fn load_error_variants_carry_diagnostic_information() {
    let e = LoadError::RootNotFound(PathBuf::from("/nowhere"));
    assert!(e.to_string().contains("/nowhere"));

    let e = LoadError::Io {
        path: PathBuf::from("/a.rs"),
        message: "disk full".into(),
    };
    assert!(e.to_string().contains("/a.rs"));
    assert!(e.to_string().contains("disk full"));

    let e = LoadError::DecodeError {
        path: PathBuf::from("/b.rs"),
        message: "invalid byte".into(),
    };
    assert!(e.to_string().contains("invalid byte"));

    let e = LoadError::Refused("not a workspace".into());
    assert!(e.to_string().contains("not a workspace"));
}
