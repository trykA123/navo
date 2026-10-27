use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use md5::{Digest, Md5};
use rusty_core::{FsError, FsErrorKind, Location, ProviderPath};
use sha2::Sha256;

use crate::{CancelToken, JobKind, JobSlot, Pools, Priority, volume_of};

const CHUNK: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algo {
    Crc32,
    Md5,
    Sha256,
}

pub fn hash_file(
    path: &Path,
    algo: Algo,
    cancel: &CancelToken,
    progress: impl FnMut(u64) + Send + 'static,
) -> Result<String, FsError> {
    let location =
        Location::local(path_name(path)).map_err(|_| FsError::new(FsErrorKind::NameInvalid))?;
    check_cancelled(cancel, &location)?;
    let queue = Pools::shared().queue(JobKind::Hash);
    let owner = queue.owner();
    let slot = JobSlot {
        volume: volume_of(&location),
        priority: Priority::Visible,
        owner: owner.id(),
    };
    let (send, receive) = mpsc::channel();
    let path = path.to_owned();
    let signal = cancel.clone();
    let source = location.clone();
    queue.submit(
        slot,
        Box::new(move |_| {
            let result = hash_path(&path, algo, &signal, progress).map_err(|mut error| {
                error.location = Some(source);
                error
            });
            let _ = send.send(result);
        }),
    );
    loop {
        check_cancelled(cancel, &location)?;
        match receive.recv_timeout(Duration::from_millis(25)) {
            Ok(result) => return result,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                let mut error = FsError::new(FsErrorKind::Unknown);
                error.location = Some(location);
                return Err(error);
            }
        }
    }
}

fn hash_path(
    path: &Path,
    algo: Algo,
    cancel: &CancelToken,
    mut progress: impl FnMut(u64),
) -> Result<String, FsError> {
    let mut file = File::open(path).map_err(io_error)?;
    let mut digest = HashState::new(algo);
    let mut buffer = vec![0; CHUNK];
    let mut bytes = 0;
    progress(bytes);
    loop {
        if cancel.is_cancelled() {
            return Err(FsError::new(FsErrorKind::Cancelled));
        }
        let count = match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io_error(error)),
        };
        digest.update(&buffer[..count]);
        bytes += count as u64;
        progress(bytes);
    }
    Ok(digest.finish())
}

fn check_cancelled(cancel: &CancelToken, location: &Location) -> Result<(), FsError> {
    if !cancel.is_cancelled() {
        return Ok(());
    }
    let mut error = FsError::new(FsErrorKind::Cancelled);
    error.location = Some(location.clone());
    Err(error)
}

fn io_error(error: io::Error) -> FsError {
    let code = error.raw_os_error();
    #[cfg(windows)]
    let kind = code.map(|code| FsErrorKind::from_windows_code(code.cast_unsigned()));
    #[cfg(not(windows))]
    let kind = code.map(FsErrorKind::from_errno);
    let mut result = FsError::new(kind.unwrap_or(FsErrorKind::Unknown));
    result.os_code = code;
    result
}

#[cfg(unix)]
fn path_name(path: &Path) -> ProviderPath {
    use std::os::unix::ffi::OsStrExt;
    ProviderPath::from_bytes(path.as_os_str().as_bytes())
}

#[cfg(windows)]
fn path_name(path: &Path) -> ProviderPath {
    use std::os::windows::ffi::OsStrExt;
    ProviderPath::from_wide(&path.as_os_str().encode_wide().collect::<Vec<_>>())
}

enum HashState {
    Crc32(crc32fast::Hasher),
    Md5(Md5),
    Sha256(Sha256),
}

impl HashState {
    fn new(algo: Algo) -> Self {
        match algo {
            Algo::Crc32 => Self::Crc32(crc32fast::Hasher::new()),
            Algo::Md5 => Self::Md5(Md5::new()),
            Algo::Sha256 => Self::Sha256(Sha256::new()),
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::Crc32(digest) => digest.update(bytes),
            Self::Md5(digest) => digest.update(bytes),
            Self::Sha256(digest) => digest.update(bytes),
        }
    }

    fn finish(self) -> String {
        match self {
            Self::Crc32(digest) => format!("{:08x}", digest.finalize()),
            Self::Md5(digest) => format!("{:x}", digest.finalize()),
            Self::Sha256(digest) => format!("{:x}", digest.finalize()),
        }
    }
}

#[cfg(test)]
mod tests;
