// Golden-example violation: domain-layer code using println!.
// The `no_stdout_in_library_code` Architecture rule must flag this.

pub fn announce(message: &str) {
    println!("domain says: {}", message);
}
