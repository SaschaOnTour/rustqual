// Golden-example violation: a domain-layer file that imports from `tokio::`.
// The `no_tokio_in_domain` Architecture pattern rule must flag this.

use tokio::spawn;

pub fn run() {
    let _handle = spawn(async {});
}
