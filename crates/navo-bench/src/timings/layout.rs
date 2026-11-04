use std::path::{Path, PathBuf};

use super::error::TimingError;
use crate::guard::ScratchRoot;
use crate::measure::Names;

pub(super) const ROOT: &str = "timings";
pub(super) const OUT: &str = "timings/out";
pub(super) const PROFILES: &str = "timings/profiles";
pub(super) const METRICS: &str = "timings/metrics";
pub(super) const TRACES: &str = "timings/traces";
pub(super) const GENERATED: &str = "timings/gen";
const SEED: u64 = 0x5eed_0a0c;

pub(super) fn bench_out(run: u32) -> PathBuf {
    Path::new(OUT).join(format!("b{run}.json"))
}

pub(super) fn profile(run: u32) -> PathBuf {
    Path::new(PROFILES).join(format!("p{run}"))
}

pub(super) fn metrics(run: u32) -> PathBuf {
    Path::new(METRICS).join(format!("m{run}.json"))
}

pub(super) fn trace(run: u32) -> PathBuf {
    Path::new(TRACES).join(format!("t{run}.jsonl"))
}

/// Creates the working folders, only through the guard.
pub(super) fn build(root: &ScratchRoot) -> Result<(), TimingError> {
    for folder in [ROOT, OUT, PROFILES, METRICS, TRACES] {
        root.create_dir(Path::new(folder))?;
    }
    Ok(())
}

/// Creates `files` empty files in one new folder, only through the guard.
pub(super) fn generate(root: &ScratchRoot, files: u32) -> Result<(), TimingError> {
    root.create_dir(Path::new(GENERATED))?;
    let mut names = Names::new(SEED);
    for index in 0..files {
        let name = names.file(usize::try_from(index).unwrap_or(usize::MAX));
        root.create_new(&Path::new(GENERATED).join(name))?;
    }
    Ok(())
}
