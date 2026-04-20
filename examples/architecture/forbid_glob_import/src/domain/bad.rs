// Golden-example violation: a domain-layer file using a glob import.
// The `no_glob_imports_in_domain` Architecture pattern rule must flag this.

use some_crate::*;

pub fn run() {
    let _ = Thing::default();
}
