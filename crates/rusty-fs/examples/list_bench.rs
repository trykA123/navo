#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a benchmark prints its timings"
)]

use std::ops::ControlFlow;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use rusty_core::{FsError, Location};
use rusty_fs::{CancelToken, EntryBatch, LocalFs, Provider};

const DEFAULT_RUNS: usize = 5;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(text) = args.next() else {
        eprintln!("usage: list_bench <folder> [runs]");
        return ExitCode::FAILURE;
    };
    let runs = args
        .next()
        .and_then(|runs| runs.parse().ok())
        .unwrap_or(DEFAULT_RUNS)
        .max(1);
    let folder = match Location::new(text.as_str()) {
        Ok(folder) => folder,
        Err(e) => {
            eprintln!("{text}: {e}");
            return ExitCode::FAILURE;
        }
    };
    match bench(&folder, runs) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("listing {text} failed: {e:?}");
            ExitCode::FAILURE
        }
    }
}

fn bench(folder: &Location, runs: usize) -> Result<(), FsError> {
    let listers = [
        ("read_dir", LocalFs::new().with_std_listing()),
        ("native", LocalFs::new()),
    ];
    let mut times: [Vec<Duration>; 2] = [Vec::new(), Vec::new()];
    let mut counts = [0; 2];
    for round in 0..=runs {
        for (index, (_, lister)) in listers.iter().enumerate() {
            let (elapsed, count) = time_listing(lister, folder)?;
            counts[index] = count;
            if round > 0 {
                times[index].push(elapsed);
            }
        }
    }
    println!(
        "{} ({runs} runs after one warm-up)",
        folder.path().to_text()
    );
    for (index, (name, _)) in listers.iter().enumerate() {
        let median = median_ms(&mut times[index]);
        println!(
            "  {name:>8}: median {median:.1} ms, {} entries",
            counts[index]
        );
    }
    Ok(())
}

fn time_listing(lister: &LocalFs, folder: &Location) -> Result<(Duration, usize), FsError> {
    let mut count = 0;
    let mut sink = |batch: EntryBatch| {
        count += batch.entries.len();
        ControlFlow::Continue(())
    };
    let start = Instant::now();
    lister.list(folder, &mut sink, &CancelToken::new())?;
    Ok((start.elapsed(), count))
}

fn median_ms(times: &mut [Duration]) -> f64 {
    times.sort_unstable();
    times
        .get(times.len() / 2)
        .map_or(0.0, |median| median.as_secs_f64() * 1000.0)
}
