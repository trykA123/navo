use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

/// Why a finished install does not run the new image; the elevated run exits with code 3.
#[derive(Debug, thiserror::Error)]
pub(super) enum Mismatch {
    #[error(
        "the service image differs from this Navo (size {found} vs {wanted}, sha256 {found_hash} vs {wanted_hash})"
    )]
    Image {
        found: u64,
        wanted: u64,
        found_hash: String,
        wanted_hash: String,
    },
    #[error("the service is registered as {0}, not the new image")]
    Command(String),
    #[error("the service is not running after the install")]
    NotRunning,
    #[error("the service runs {0}, not the new image")]
    OtherImage(PathBuf),
    #[error("the service process started before the new image was in place")]
    OldProcess,
}

/// What the SCM reports once the service was started from the new image.
pub(super) struct Running<'a> {
    pub command: &'a OsStr,
    pub process: Option<Process>,
}

pub(super) struct Process {
    pub image: PathBuf,
    /// Creation time in 100 ns ticks (FILETIME).
    pub created: u64,
}

/// The registered command, the image path and the earliest start time a new process can have.
pub(super) struct Expected<'a> {
    pub command: &'a OsStr,
    pub image: &'a Path,
    pub started_after: u64,
}

/// The installed image must have the size and SHA-256 of the exe that ran the install.
pub(super) fn check_image(source: &Path, image: &Path) -> io::Result<()> {
    let facts = |path: &Path| -> io::Result<(u64, String)> {
        let size = std::fs::metadata(path)?.len();
        let cancel = navo_fs::CancelToken::new();
        let hash = navo_fs::hash::hash_file(path, navo_fs::hash::Algo::Sha256, &cancel, |_| {})
            .map_err(|error| io::Error::other(format!("hashing {}: {error:?}", path.display())))?;
        Ok((size, hash))
    };
    let (wanted, wanted_hash) = facts(source)?;
    let (found, found_hash) = facts(image)?;
    if (found, &found_hash) == (wanted, &wanted_hash) {
        return Ok(());
    }
    Err(io::Error::other(Mismatch::Image {
        found,
        wanted,
        found_hash,
        wanted_hash,
    }))
}

/// The service must be registered with the new command and run a process started after it.
pub(super) fn check_running(expected: &Expected, running: &Running) -> Result<(), Mismatch> {
    if !same_text(running.command, expected.command) {
        return Err(Mismatch::Command(
            running.command.to_string_lossy().into_owned(),
        ));
    }
    let process = running.process.as_ref().ok_or(Mismatch::NotRunning)?;
    if !same_text(process.image.as_os_str(), expected.image.as_os_str()) {
        return Err(Mismatch::OtherImage(process.image.clone()));
    }
    if process.created < expected.started_after {
        return Err(Mismatch::OldProcess);
    }
    Ok(())
}

/// Windows paths and service commands compare without case.
fn same_text(left: &OsStr, right: &OsStr) -> bool {
    left.to_string_lossy().to_lowercase() == right.to_string_lossy().to_lowercase()
}

pub(super) fn is_mismatch(error: &io::Error) -> bool {
    error.get_ref().is_some_and(|inner| inner.is::<Mismatch>())
}
