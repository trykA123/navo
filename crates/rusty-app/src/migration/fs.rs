use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read as _};
use std::path::Path;

const CHUNK: usize = 64 * 1024;

pub(crate) trait MigrationFs {
    fn exists(&self, path: &Path) -> bool;
    fn entries(&self, path: &Path) -> io::Result<Vec<OsString>>;
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn same_bytes(&self, a: &Path, b: &Path) -> io::Result<bool>;
    fn create_dir(&self, path: &Path) -> io::Result<()>;
    fn copy(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn write_file(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;
    fn sync_file(&self, path: &Path) -> io::Result<()>;
    fn sync_dir(&self, path: &Path) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn replace(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn remove_dir(&self, path: &Path) -> io::Result<()>;
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
}

pub(crate) struct RealFs;

impl MigrationFs for RealFs {
    fn exists(&self, path: &Path) -> bool {
        fs::symlink_metadata(path).is_ok()
    }

    fn entries(&self, path: &Path) -> io::Result<Vec<OsString>> {
        fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect()
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn same_bytes(&self, a: &Path, b: &Path) -> io::Result<bool> {
        let (mut a, mut b) = (File::open(a)?, File::open(b)?);
        if a.metadata()?.len() != b.metadata()?.len() {
            return Ok(false);
        }
        let (mut left, mut right) = (vec![0; CHUNK], vec![0; CHUNK]);
        loop {
            let n = read_full(&mut a, &mut left)?;
            let m = read_full(&mut b, &mut right)?;
            if left[..n] != right[..m] {
                return Ok(false);
            }
            if n == 0 {
                return Ok(true);
            }
        }
    }

    fn create_dir(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn copy(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut source = File::open(from)?;
        let mut target = OpenOptions::new().write(true).create_new(true).open(to)?;
        io::copy(&mut source, &mut target).map(drop)
    }

    fn write_file(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        fs::write(path, bytes)
    }

    fn sync_file(&self, path: &Path) -> io::Result<()> {
        OpenOptions::new().write(true).open(path)?.sync_all()
    }

    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        sync_folder(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.exists(to) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} already exists", to.display()),
            ));
        }
        fs::rename(from, to)
    }

    fn replace(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir(path)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }
}

fn read_full(file: &mut File, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match file.read(&mut buffer[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

#[cfg(unix)]
fn sync_folder(folder: &Path) -> io::Result<()> {
    File::open(folder)?.sync_all()
}

#[cfg(windows)]
fn sync_folder(folder: &Path) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;

    let synced = OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(folder)
        .and_then(|folder| folder.sync_all());
    match synced {
        Err(error) if error.raw_os_error() == i32::try_from(ERROR_ACCESS_DENIED).ok() => Ok(()),
        other => other,
    }
}
