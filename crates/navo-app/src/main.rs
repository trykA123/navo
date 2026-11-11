//! Navo's executable: picks the subsystem and the allocator, then runs the app.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(all(feature = "mimalloc", not(feature = "tester")))]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> std::process::ExitCode {
    navo::run()
}
