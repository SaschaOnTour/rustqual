fn main() {
    if let Err(code) = rustqual::run() {
        std::process::exit(code);
    }
}
