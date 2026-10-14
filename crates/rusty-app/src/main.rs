#![allow(clippy::print_stdout, reason = "prints the version for --version")]

fn main() {
    println!("ferrite {}", env!("CARGO_PKG_VERSION"));
}
