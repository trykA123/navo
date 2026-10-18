use std::fs::{self, OpenOptions};
use std::path::Path;

pub(super) fn generate(root: &Path) -> Result<(), String> {
    let left = root.join("A");
    let right = root.join("B");
    files(&left, 5_000)?;
    for index in 0..200 {
        files(&right.join(format!("folder-{index:03}")), 25)?;
    }
    Ok(())
}

fn files(folder: &Path, count: usize) -> Result<(), String> {
    fs::create_dir_all(folder).map_err(|error| format!("{}: {error}", folder.display()))?;
    for index in 0..count {
        let path = folder.join(format!("file-{index:04}.txt"));
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(())
}
