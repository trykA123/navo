use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub(super) fn regular(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => Err(io::Error::other(format!(
            "{} must be a regular file",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub(super) fn read(path: &Path, limit: u64) -> io::Result<Option<Vec<u8>>> {
    if !regular(path)? {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::other(format!(
            "{} exceeds the recovery record limit",
            path.display()
        )));
    }
    Ok(Some(bytes))
}

pub(super) fn replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    regular(path)?;
    rusty_store::replace_atomic(path, bytes)
}

pub(super) fn create(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

pub(super) fn suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    name.into()
}
