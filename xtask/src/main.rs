#![deny(unsafe_code)]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "xtask is a CLI that reports to the terminal"
)]

mod bench_copy;
mod bench_diff;
mod bench_large;
mod bench_walk;
mod binsize;
mod colors;
mod coverage;
mod gate;
mod l10n;
mod perf;
mod size;
pub mod stats;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("size") => size::run(rest),
        Some("coverage") => coverage::run(rest),
        Some("binsize") => binsize::run(rest),
        Some("gate") => gate::run(rest),
        Some("check") => gate::check(rest),
        Some("l10n") => l10n::run(rest),
        Some("colors") => colors::run(rest),
        Some("perf-fixture") => perf::fixture(rest),
        Some("perf-summary") => perf::summary(rest),
        Some("bench-copy") => bench_copy::run(rest),
        Some("bench-diff") => bench_diff::run(rest),
        Some("bench-large") => bench_large::run(rest),
        Some("bench-walk") => bench_walk::run(rest),
        _ => {
            eprintln!(
                "usage: cargo xtask <size | coverage | binsize <path> | gate [--coverage] | check -p <crate> [-p <crate>...] | l10n [--list] | colors | perf-fixture <dir> | perf-summary <reports-dir> <summary-file> | bench-copy <target-folder> [--runs N] [--sets small,medium,large] | bench-diff [--quick] [--json <path>] | bench-large [--quick] [--huge] [--json <path>] | bench-walk <dir> [--runs N] [--warmup N] [--prepare \"<command>\"] [--targets list,size]>"
            );
            ExitCode::from(2)
        }
    }
}
