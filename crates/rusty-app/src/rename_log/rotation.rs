use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub(super) const MAX_BYTES: u64 = 10 * 1024 * 1024;

pub(super) fn append(path: &Path, batch: &[u8]) -> Result<(), String> {
    if batch.len() as u64 > MAX_BYTES {
        return Err("rename log batch exceeds the size limit".to_owned());
    }
    let lock_path = related(path, ".lock");
    size(&lock_path)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .map_err(|error| error.to_string())?;
    lock.lock().map_err(|error| error.to_string())?;
    let bytes = size(path)?;
    if bytes > MAX_BYTES.saturating_sub(batch.len() as u64) {
        rotate(path)?;
    }
    OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(path)
        .and_then(|mut file| file.write_all(batch).and_then(|()| file.sync_data()))
        .map_err(|error| error.to_string())
}

fn size(path: &Path) -> Result<u64, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => Ok(meta.len()),
        Ok(_) => Err("rename log path is not a regular file".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.to_string()),
    }
}

fn rotate(path: &Path) -> Result<(), String> {
    recognized(path)?;
    let backup = related(path, ".1");
    match fs::symlink_metadata(&backup) {
        Ok(_) => {
            size(&backup)?;
            recognized(&backup)?;
            fs::remove_file(&backup).map_err(|error| error.to_string())?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    place_backup(path, &backup)
}

fn place_backup(path: &Path, backup: &Path) -> Result<(), String> {
    use rusty_fs::ProviderOps;
    rusty_fs::LocalFs::new()
        .rename_atomic(&native_location(path)?, &native_location(backup)?)
        .map_err(|error| error.message())
}

fn native_location(path: &Path) -> Result<rusty_core::Location, String> {
    let path = std::path::absolute(path).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    let name = {
        use std::os::unix::ffi::OsStrExt;
        rusty_core::ProviderPath::from_bytes(path.as_os_str().as_bytes())
    };
    #[cfg(windows)]
    let name = {
        use std::os::windows::ffi::OsStrExt;
        rusty_core::ProviderPath::from_wide(&path.as_os_str().encode_wide().collect::<Vec<_>>())
    };
    rusty_core::Location::local(name).map_err(|error| error.to_string())
}

fn recognized(path: &Path) -> Result<(), String> {
    let mut prefix = [0_u8; 64];
    let read = File::open(path)
        .and_then(|mut file| file.read(&mut prefix))
        .map_err(|error| error.to_string())?;
    if !prefix[..read].starts_with(super::FORMAT.as_bytes())
        && !prefix[..read].starts_with(b"# rename batch ")
    {
        return Err("refusing to rotate an unrecognized rename log".to_owned());
    }
    Ok(())
}

fn related(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

#[cfg(test)]
mod tests;
