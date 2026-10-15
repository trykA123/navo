use std::fs::{self, File};
use std::io::{Read, Write};

use rusty_core::{FsError, FsErrorKind, Location};

use super::{LocalFs, error, io_error, os_path};
use crate::{CancelToken, ProviderOps};

const CHUNK: usize = 1 << 20;

impl ProviderOps for LocalFs {
    fn create_folder(&self, location: &Location) -> Result<(), FsError> {
        let path = os_path(location)?;
        fs::create_dir(&path).map_err(|e| io_error(&e, location))
    }

    fn copy_file(
        &self,
        source: &Location,
        target: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError> {
        let mut input = File::open(os_path(source)?).map_err(|e| io_error(&e, source))?;
        let metadata = input.metadata().map_err(|e| io_error(&e, source))?;
        let target_path = os_path(target)?;
        let mut output = File::options()
            .write(true)
            .create_new(true)
            .open(&target_path)
            .map_err(|e| io_error(&e, target))?;
        let total = pump(&mut input, &mut output, progress, cancel)
            .map_err(|failure| failure.located(source, target))?;
        output.sync_all().map_err(|e| io_error(&e, target))?;
        if let Ok(modified) = metadata.modified() {
            let _ = output.set_modified(modified);
        }
        drop(output);
        if metadata.permissions().readonly() {
            let mut permissions = metadata.permissions();
            permissions.set_readonly(true);
            let _ = fs::set_permissions(&target_path, permissions);
        }
        Ok(total)
    }

    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        let target_path = os_path(target)?;
        if fs::symlink_metadata(&target_path).is_ok() {
            return Err(error(FsErrorKind::Unknown, target));
        }
        fs::rename(os_path(source)?, &target_path).map_err(|e| io_error(&e, source))
    }

    fn remove_file(&self, location: &Location) -> Result<(), FsError> {
        fs::remove_file(os_path(location)?).map_err(|e| io_error(&e, location))
    }

    fn remove_link(&self, location: &Location) -> Result<(), FsError> {
        let path = os_path(location)?;
        let metadata = fs::symlink_metadata(&path).map_err(|e| io_error(&e, location))?;
        if !metadata.file_type().is_symlink() {
            return Err(error(FsErrorKind::NameInvalid, location));
        }
        fs::remove_file(&path)
            .or_else(|_| fs::remove_dir(&path))
            .map_err(|e| io_error(&e, location))
    }

    fn remove_empty_folder(&self, location: &Location) -> Result<(), FsError> {
        fs::remove_dir(os_path(location)?).map_err(|e| io_error(&e, location))
    }

    fn recycle(&self, location: &Location) -> Result<(), FsError> {
        trash::delete(os_path(location)?).map_err(|_| error(FsErrorKind::Unknown, location))
    }
}

enum PumpError {
    Read(std::io::Error),
    Write(std::io::Error),
    Cancelled,
}

impl PumpError {
    fn located(self, source: &Location, target: &Location) -> FsError {
        match self {
            Self::Read(e) => io_error(&e, source),
            Self::Write(e) => io_error(&e, target),
            Self::Cancelled => error(FsErrorKind::Cancelled, target),
        }
    }
}

fn pump(
    input: &mut File,
    output: &mut File,
    progress: &mut dyn FnMut(u64),
    cancel: &CancelToken,
) -> Result<u64, PumpError> {
    let mut buffer = vec![0_u8; CHUNK];
    let mut total = 0_u64;
    loop {
        if cancel.is_cancelled() {
            return Err(PumpError::Cancelled);
        }
        let read = input.read(&mut buffer).map_err(PumpError::Read)?;
        if read == 0 {
            return Ok(total);
        }
        let chunk = buffer.get(..read).unwrap_or_default();
        output.write_all(chunk).map_err(PumpError::Write)?;
        total += read as u64;
        progress(total);
    }
}
