#![allow(clippy::print_stdout, reason = "prints the version for --version")]

fn main() {
    println!("rusty {}", env!("CARGO_PKG_VERSION"));
}
