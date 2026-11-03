use std::path::PathBuf;
use std::time::Instant;

use navo_git::status::GitStatus;
use navo_git::{Cancel, CliGit, GixGit};

/// Times gix and git status on the repository in `NAVO_MEASURE_REPO`; run with `--run-ignored only --no-capture`.
#[test]
#[ignore = "manual measurement: set NAVO_MEASURE_REPO"]
#[allow(
    clippy::print_stderr,
    reason = "a manual measurement reports on stderr"
)]
fn measure_gix_against_git_status() {
    let Some(path) = std::env::var_os("NAVO_MEASURE_REPO").map(PathBuf::from) else {
        eprintln!("skipped: NAVO_MEASURE_REPO is not set");
        return;
    };
    let cancel = Cancel::default();
    let gix = GixGit::new(&path).unwrap();
    let guard = gix.guard().unwrap();
    let mut times = Vec::new();
    for _ in 0..5 {
        let started = Instant::now();
        let status = gix.status(&guard, &cancel).unwrap();
        times.push((started.elapsed().as_millis(), status.changed));
    }
    times.sort_unstable();
    let (median, changed) = times[times.len() / 2];
    eprintln!("gix: median {median} ms, changed {changed}, runs {times:?}");
    match CliGit::new(&path) {
        Ok(git) => {
            let guard = git.guard().unwrap();
            let started = Instant::now();
            let status = git.status(&guard, &cancel).unwrap();
            eprintln!(
                "git: {} ms, changed {}",
                started.elapsed().as_millis(),
                status.changed
            );
        }
        Err(error) => eprintln!("git: not available ({error})"),
    }
}
