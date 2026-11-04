use std::path::{Path, PathBuf};

use super::host::MeasureError;
use super::names::Names;
use crate::guard::ScratchRoot;

pub(super) const FIXTURES: &str = "fixtures";
pub(super) const FIXTURE_A: &str = "fixtures/a";
pub(super) const FIXTURE_B: &str = "fixtures/b";
pub(super) const PROFILES: &str = "profiles";
pub(super) const METRICS: &str = "metrics";
pub(super) const EXES: &str = "exe";
pub(super) const A_FILES: usize = 5_000;
pub(super) const B_FOLDERS: usize = 200;
pub(super) const B_FILES_PER_FOLDER: usize = 25;
const SEED: u64 = 0x5eed_0a0b;

pub(super) fn profile(run: u32) -> PathBuf {
    Path::new(PROFILES).join(format!("p{run}"))
}

pub(super) fn metrics(run: u32) -> PathBuf {
    Path::new(METRICS).join(format!("m{run}.json"))
}

pub(super) fn exe_folder(copy: u32) -> PathBuf {
    Path::new(EXES).join(format!("e{copy}"))
}

/// Creates the working folders and fixtures A and B, only through the guard.
pub(super) fn build(root: &ScratchRoot) -> Result<(), MeasureError> {
    for folder in [FIXTURES, PROFILES, METRICS, EXES, FIXTURE_A, FIXTURE_B] {
        root.create_dir(Path::new(folder))?;
    }
    let mut names = Names::new(SEED);
    for index in 0..A_FILES {
        touch(root, &Path::new(FIXTURE_A).join(names.file(index)))?;
    }
    for folder_index in 0..B_FOLDERS {
        let folder = Path::new(FIXTURE_B).join(names.folder(folder_index));
        root.create_dir(&folder)?;
        for index in 0..B_FILES_PER_FOLDER {
            touch(root, &folder.join(names.file(index)))?;
        }
    }
    Ok(())
}

fn touch(root: &ScratchRoot, rel: &Path) -> Result<(), MeasureError> {
    root.create_new(rel).map(drop).map_err(MeasureError::from)
}
