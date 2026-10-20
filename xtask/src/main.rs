#![forbid(unsafe_code)]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "xtask is a CLI that reports to the terminal"
)]

mod bench_copy;
mod binsize;
mod coverage;
mod gate;
mod l10n;
mod perf;
mod size;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("size") => size::run(rest),
        Some("coverage") => coverage::run(rest),
        Some("binsize") => binsize::run(rest),
        Some("gate") => gate::run(rest),
        Some("l10n") => l10n::run(rest),
        Some("perf-fixture") => perf::fixture(rest),
        Some("perf-summary") => perf::summary(rest),
        Some("bench-copy") => bench_copy::run(rest),
        _ => {
            eprintln!(
                "usage: cargo xtask <size | coverage | binsize <path> | gate [--coverage] | l10n [--list] | perf-fixture <dir> | perf-summary <reports-dir> <summary-file> | bench-copy <target-folder> [--runs N] [--sets small,medium,large]>"
            );
            ExitCode::from(2)
        }
    }
}
