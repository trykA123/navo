//! Build guard for the tester trace build: a navo-diag built with the `tracy` feature must
//! force-include the `navo@tester` header into the Tracy client, and refuse every other way
//! those C++ flags could be set (SC14).

#![allow(
    clippy::print_stdout,
    reason = "cargo reads build script directives from stdout"
)]

include!("tracy/flags.rs");

/// Asks to be rerun whenever a flag source changes, then refuses a trace build whose flags are
/// not exactly one force-include of the guard header.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target = std::env::var("TARGET").unwrap_or_default();
    for name in watched_vars(&target) {
        println!("cargo:rerun-if-env-changed={name}");
    }
    println!("cargo:rerun-if-changed=tracy/flags.rs");
    if std::env::var_os("CARGO_FEATURE_TRACY").is_none() {
        return Ok(());
    }
    let get = |name: &str| std::env::var(name).ok();
    let exists = |path: &std::path::Path| path.is_file();
    check_env(&target, &get, &exists)?;
    Ok(())
}
