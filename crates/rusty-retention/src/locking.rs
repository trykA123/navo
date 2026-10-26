use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;

pub(super) fn claim(profile: &Path) -> io::Result<File> {
    let path = profile.join("retention.lock");
    match fs::symlink_metadata(&path) {
        Ok(metadata) if !metadata.is_file() => {
            return Err(io::Error::other("retention lock is not a regular file"));
        }
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => (),
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock().map_err(io::Error::from)?;
    Ok(file)
}
